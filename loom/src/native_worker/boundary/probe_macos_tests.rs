//! The macOS qualification gates and verdicts, over fake hosts and fixtures:
//! pure, so they run on every platform. The kernel-dependent probes are in
//! `probe_mac_kernel_tests.rs`.
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use serde_json::json;

use super::judge::{
    classify_sandbox_exec, judge_codex_home, judge_payload, judge_untouched, response_to,
    take_line, PAYLOAD_OK, PAYLOAD_SCRIPT, SANDBOX_REFUSAL,
};
use super::{qualify_macos_from, MacHostProbes, UNSUPPORTED_HOST};

/// A host whose probes record the order they are asked in.
struct Recording(RefCell<Vec<&'static str>>);

impl MacHostProbes for Recording {
    fn sandbox_exec(&self) -> Result<(), String> {
        self.0.borrow_mut().push("sandbox_exec");
        Ok(())
    }

    fn payload_boundary(&self) -> Result<(), String> {
        self.0.borrow_mut().push("payload_boundary");
        Err("the payload escaped".into())
    }

    fn codex_home_honoured(&self) -> Result<(), String> {
        self.0.borrow_mut().push("codex_home_honoured");
        Ok(())
    }
}

#[test]
fn a_failing_payload_probe_is_the_answer_and_the_codex_probe_never_runs() {
    let host = Recording(RefCell::new(Vec::new()));
    assert_eq!(
        qualify_macos_from(&host),
        Err("the payload escaped".to_string())
    );
    assert_eq!(*host.0.borrow(), ["sandbox_exec", "payload_boundary"]);
}

#[test]
fn an_unsupported_host_is_refused_naming_both_supported_platforms() {
    assert_eq!(UNSUPPORTED_HOST, "native Codex workers need Linux or macOS");
}

#[test]
fn a_nested_sandbox_refusal_tells_the_operator_to_start_from_a_plain_shell() {
    let nested = "/usr/bin/sandbox-exec failed (exit status: 71: sandbox-exec: sandbox_apply: \
                  Operation not permitted)";
    let reason = classify_sandbox_exec(nested);
    assert!(reason.contains(SANDBOX_REFUSAL), "{reason}");
    assert!(reason.contains(nested), "the raw refusal stays: {reason}");
    let missing = classify_sandbox_exec("/usr/bin/sandbox-exec not found");
    assert!(!missing.contains(SANDBOX_REFUSAL), "{missing}");
    assert!(missing.contains("sandbox-exec not found"), "{missing}");
}

#[test]
fn the_payload_verdict_names_what_the_boundary_got_wrong() {
    assert_eq!(judge_payload(Some(0), &format!("{PAYLOAD_OK}\n")), Ok(()));
    let cases = [
        (Some(21), "worktree is not writable"),
        (
            Some(22),
            "write outside the boundary's roots was not denied",
        ),
        (Some(23), "read outside the boundary's roots was not denied"),
        (Some(0), "did not report success"),
        (Some(7), "exited 7"),
        (None, "killed by a signal"),
    ];
    for (code, expected) in cases {
        let refusal = judge_payload(code, "something else\n").unwrap_err();
        assert!(refusal.contains(expected), "{code:?}: {refusal}");
    }
}

fn payload(worktree: &Path, host_home: &Path) -> (Option<i32>, String) {
    let output = Command::new("/bin/sh")
        .args(["-c", PAYLOAD_SCRIPT, "sh"])
        .arg(worktree)
        .arg(host_home)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// Without a boundary the payload reports its escape (exit 22); with the host
/// home closed by file permissions instead it reports success: the script's
/// verdicts follow the boundary, not the shell.
#[test]
fn the_payload_script_distinguishes_a_closed_host_home_from_an_open_one() {
    let dir = tempfile::tempdir().unwrap();
    let (worktree, home) = (dir.path().join("worktree"), dir.path().join("home"));
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("secret"), "s").unwrap();
    assert_eq!(payload(&dir.path().join("none"), &home).0, Some(21));
    assert_eq!(payload(&worktree, &home).0, Some(22), "an open host home");
    std::fs::remove_file(home.join("denied-write")).unwrap();

    std::fs::set_permissions(home.join("secret"), std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500)).unwrap();
    let outcome = payload(&worktree, &home);
    let closed = std::fs::write(home.join("probe"), "x").is_err();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    if closed {
        assert_eq!(outcome, (Some(0), format!("{PAYLOAD_OK}\n")));
    }
    assert!(worktree.join("allowed").is_file());
}

#[test]
fn a_response_is_matched_by_id_and_an_error_is_its_message() {
    let ok = r#"{"id":2,"result":{"thread":{"id":"t"}}}"#;
    assert_eq!(response_to(ok, 2), Some(Ok(json!({"thread": {"id": "t"}}))));
    assert_eq!(response_to(ok, 1), None, "another request's response");
    assert_eq!(
        response_to(r#"{"method":"remoteControl/status/changed"}"#, 2),
        None
    );
    assert_eq!(response_to("not json", 2), None);
    let refused = r#"{"id":1,"error":{"code":-32600,"message":"Not initialized"}}"#;
    assert_eq!(response_to(refused, 1), Some(Err("Not initialized".into())));
}

#[test]
fn lines_are_taken_whole_and_a_partial_line_waits() {
    let mut buffer = b"one\ntwo\npart".to_vec();
    assert_eq!(take_line(&mut buffer).as_deref(), Some("one"));
    assert_eq!(take_line(&mut buffer).as_deref(), Some("two"));
    assert_eq!(take_line(&mut buffer), None);
    assert_eq!(buffer, b"part");
}

struct Homes {
    _dir: tempfile::TempDir,
    cache: std::path::PathBuf,
    host: std::path::PathBuf,
    private: std::path::PathBuf,
}

fn homes() -> Homes {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (cache, host) = (root.join("cache"), root.join("home"));
    let private = cache.join("native/inv/codex-home");
    std::fs::create_dir_all(private.join("sessions")).unwrap();
    std::fs::create_dir_all(host.join(".codex")).unwrap();
    Homes {
        _dir: dir,
        cache,
        host,
        private,
    }
}

fn thread_at(path: &Path) -> serde_json::Value {
    json!({"thread": {"path": path.join("2026/11/21/rollout.jsonl")}})
}

#[test]
fn codex_must_report_the_controlled_home_and_keep_its_sessions_there() {
    let h = homes();
    let initialize = json!({"codexHome": h.private});
    let sessions = h.private.join("sessions");
    let reported = judge_codex_home(&initialize, &thread_at(&sessions), &h.cache, &h.host);
    assert_eq!(reported, Ok(h.private.clone()));

    let in_host = json!({"codexHome": h.host.join(".codex")});
    let host_sessions = h.host.join(".codex/sessions");
    let refusal = judge_codex_home(&in_host, &thread_at(&host_sessions), &h.cache, &h.host);
    assert!(refusal.unwrap_err().contains("inside the host home"));

    let elsewhere = json!({"codexHome": "/var/empty/.codex"});
    let refusal = judge_codex_home(&elsewhere, &thread_at(&sessions), &h.cache, &h.host);
    assert!(refusal
        .unwrap_err()
        .contains("not the controlled CODEX_HOME"));

    let refusal = judge_codex_home(&json!({}), &thread_at(&sessions), &h.cache, &h.host);
    assert!(refusal.unwrap_err().contains("did not report a codexHome"));

    let stray = thread_at(&h.host.join(".codex/sessions"));
    let refusal = judge_codex_home(&initialize, &stray, &h.cache, &h.host);
    assert!(refusal
        .unwrap_err()
        .contains("not under the controlled home"));
    let refusal = judge_codex_home(&initialize, &json!({"thread": {}}), &h.cache, &h.host);
    assert!(refusal.unwrap_err().contains("no session path"));
}

#[test]
fn a_host_home_the_boundary_touched_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("secret"), "s").unwrap();
    assert_eq!(judge_untouched(dir.path(), &["secret"]), Ok(()));
    std::fs::write(dir.path().join("denied-write"), "x").unwrap();
    let refusal = judge_untouched(dir.path(), &["secret"]).unwrap_err();
    assert!(refusal.contains("denied-write"), "{refusal}");
}
