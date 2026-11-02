use super::*;
use tempfile::TempDir;

#[test]
fn rewriting_process_identity_preserves_owner_metadata() {
    let temp = TempDir::new().unwrap();
    create_pid_dir(temp.path()).unwrap();
    let key = "loom-v2-stage-auth-session-abcd1234-1";
    let path = pid_file_path(temp.path(), key);
    fs::write(&path, "123\n\nloom-owner:loom-v2-stage-auth\n").unwrap();
    write_pid_entry(
        temp.path(),
        key,
        PidEntry {
            pid: 123,
            start_time: Some(456),
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "123\n456\nloom-owner:loom-v2-stage-auth\n"
    );
}

#[test]
fn test_cleanup_stage_files() {
    use crate::models::session::SessionType;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let pid_key = "loom-test-stage-session-cleanup-1234567890";

    super::super::wrapper::create_wrapper_script(
        work_dir,
        pid_key,
        "test-stage",
        "session-cleanup-1234567890",
        "claude 'test'",
        None,
        SessionType::Stage,
        crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS,
    )
    .unwrap();

    assert!(wrapper_script_path(work_dir, pid_key).exists());
    cleanup_stage_files(work_dir, pid_key);
    assert!(!wrapper_script_path(work_dir, pid_key).exists());
}

#[test]
fn test_read_pid_entry_pid_only() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    create_pid_dir(work_dir).unwrap();
    let pid_key = "loom-stage-session-1";
    fs::write(pid_file_path(work_dir, pid_key), "12345\n").unwrap();

    let entry = read_pid_entry(work_dir, pid_key).unwrap();
    assert_eq!(entry.pid, 12345);
    assert_eq!(entry.start_time, None);
}

#[test]
fn test_read_pid_entry_with_start_time() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    create_pid_dir(work_dir).unwrap();
    let pid_key = "loom-stage-session-2";
    fs::write(pid_file_path(work_dir, pid_key), "999\n7777\n").unwrap();

    let entry = read_pid_entry(work_dir, pid_key).unwrap();
    assert_eq!(entry.pid, 999);
    assert_eq!(entry.start_time, Some(7777));
}

#[test]
fn test_read_pid_entry_missing() {
    let temp_dir = TempDir::new().unwrap();
    assert!(read_pid_entry(temp_dir.path(), "no-such-key").is_none());
}

#[test]
fn test_verify_pid_entry_dead_pid() {
    // A PID that does not exist must never match, regardless of start_time.
    let entry = PidEntry {
        pid: 999_999_999,
        start_time: Some(123),
    };
    assert_eq!(
        crate::process::verify_process_identity(entry),
        crate::process::IdentityStatus::Dead
    );
}

#[test]
fn test_verify_pid_entry_live_no_start_time_is_unverifiable() {
    let entry = PidEntry {
        pid: std::process::id(),
        start_time: None,
    };
    assert_eq!(
        crate::process::verify_process_identity(entry),
        crate::process::IdentityStatus::Unverifiable
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_verify_pid_entry_start_time_mismatch_is_reuse() {
    // Our own PID is alive, but a bogus recorded start_time means the PID
    // was "recycled" — so the entry must NOT match.
    let entry = PidEntry {
        pid: std::process::id(),
        start_time: Some(1), // never the real start-time
    };
    assert_eq!(
        crate::process::verify_process_identity(entry),
        crate::process::IdentityStatus::Dead
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_process_start_time_self_then_matches() {
    // The real recorded start-time of our own process matches on probe.
    let pid = std::process::id();
    let start = crate::process::process_start_time(pid).expect("own start-time readable on Linux");
    let entry = PidEntry {
        pid,
        start_time: Some(start),
    };
    assert_eq!(
        crate::process::verify_process_identity(entry),
        crate::process::IdentityStatus::VerifiedAlive
    );
}
