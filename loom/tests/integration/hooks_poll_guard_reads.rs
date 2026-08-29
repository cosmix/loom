//! Bash-side cat/sed/head/tail read cases for `hooks_poll_guard.rs`.
//!
//! Split out purely for size: these tests pin poll-guard.sh's reuse of read-guard.sh's rule 1
//! (a Bash-side `cat`/`head`/`tail` full read gets the same large-file treatment as the Read
//! tool), prove the two hooks share one read ledger rather than tracking reads separately, and
//! pin the head/tail byte-count-and-follow exemption plus the `sed -n '1,$p'` full-read
//! reclassification - sharing the parent's harness (hook installation, `Session`,
//! `run_bash_hook`, `warn_context`) via `use super::*` - read the parent's module docs first.

use super::*;

// 4. Bash-side reads get the same rules as the Read tool: `cat` of a big,
//    graph-covered file denies with the outline inline (switch on) / warns
//    with it off; `sed -n` of the same file as a bounded range is allowed.
#[test]
fn bash_side_cat_gets_read_rules_sed_range_is_allowed() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);
    let path = file.to_string_lossy();

    let on = Session::new().with_live_main_agent();
    on.enable_deny();
    let out = run_bash_hook(&hook, &format!("cat {path}"), &on, Some(&stub_dir));
    assert_eq!(out.code, 2, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(
        out.stderr
            .contains("Read the ranges you need with offset/limit"),
        "stderr={}",
        out.stderr
    );

    let off = Session::new();
    let out = run_bash_hook(&hook, &format!("cat {path}"), &off, Some(&stub_dir));
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        warn_context(&out.stdout).contains("Outline"),
        "stdout={}",
        out.stdout
    );

    let ranged = Session::new();
    let out = run_bash_hook(
        &hook,
        &format!("sed -n '10,40p' {path}"),
        &ranged,
        Some(&stub_dir),
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "range read must not warn: {}",
        out.stdout
    );
}

// 4b. The read ledger `cat` writes is the SAME ledger read-guard.sh reads -
//     a `cat` full read followed by a Read-tool full read of the same path
//     escalates as a repeat, proving the shared library, not a coincidence.
#[test]
fn bash_side_cat_and_read_tool_share_the_same_read_ledger() {
    let (_temp, poll_hook, read_hook) = setup_both_hooks();
    let files = TempDir::new().expect("files dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let path = file.to_string_lossy();
    let session = Session::new();

    let cat_out = run_bash_hook(&poll_hook, &format!("cat {path}"), &session, None);
    assert_eq!(cat_out.code, 0, "stderr={}", cat_out.stderr);
    assert!(
        cat_out.stdout.trim().is_empty(),
        "1st full read must be clean: {}",
        cat_out.stdout
    );

    let read_payload = json!({
        "tool_name": "Read",
        "tool_input": {"file_path": path},
        "agent_id": session.agent_id,
        "session_id": session.session_id,
    });
    let read_out = run_payload(&read_hook, &read_payload, &session, None);
    assert_eq!(read_out.code, 0, "stderr={}", read_out.stderr);
    assert!(
        warn_context(&read_out.stdout).contains("read in full at"),
        "Read after cat must see the same ledger: {}",
        read_out.stdout
    );
}

// 5. `head -n 20`/`tail -n 20` are bounded range reads; a bare `head`/`tail`
//    with no count is an unbounded full read, subject to rule 1 exactly like
//    `cat`.
#[test]
fn head_tail_with_count_is_range_bare_is_full() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);
    let path = file.to_string_lossy();

    for (bounded_cmd, bare_cmd) in [
        (format!("head -n 20 {path}"), format!("head {path}")),
        (format!("tail -n 20 {path}"), format!("tail {path}")),
    ] {
        let ranged = Session::new();
        ranged.enable_deny();
        let out = run_bash_hook(&hook, &bounded_cmd, &ranged, Some(&stub_dir));
        assert_eq!(out.code, 0, "{bounded_cmd}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "{bounded_cmd} must not warn: {}",
            out.stdout
        );

        let full = Session::new().with_live_main_agent();
        full.enable_deny();
        let out = run_bash_hook(&hook, &bare_cmd, &full, Some(&stub_dir));
        assert_eq!(
            out.code, 2,
            "{bare_cmd} must be treated as a full read: stderr={}",
            out.stderr
        );
    }
}

// New. `head -c 200 <big file>` (a byte count) and `tail -f <big file>` (a follow) are OUT OF
// SCOPE for the line-count discipline entirely, not treated as an unbounded "full" read: a byte
// count is the least wasteful read possible, and a follow never terminates. Neither warns nor
// denies even though the file is large, graph-covered, and the switch is on.
#[test]
fn head_byte_count_and_tail_follow_skip_line_discipline() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);
    let path = file.to_string_lossy();

    for cmd in [format!("head -c 200 {path}"), format!("tail -f {path}")] {
        let session = Session::new().with_live_main_agent();
        session.enable_deny();
        let out = run_bash_hook(&hook, &cmd, &session, Some(&stub_dir));
        assert_eq!(out.code, 0, "{cmd}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "{cmd} must not warn or deny: {}",
            out.stdout
        );
    }
}

// New. `sed -n '1,$p'` is a range ending at the LAST line - a "full" read of the whole file, not
// a bounded range. It gets the same large-file treatment `cat`/bare `head` get; before the fix it
// escaped rule 1 entirely by superficially looking like a bounded range.
#[test]
fn sed_dollar_range_is_treated_as_a_full_read() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);
    let path = file.to_string_lossy();

    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let out = run_bash_hook(
        &hook,
        &format!("sed -n '1,$p' {path}"),
        &session,
        Some(&stub_dir),
    );
    assert_eq!(out.code, 2, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(
        out.stderr
            .contains("Read the ranges you need with offset/limit"),
        "stderr={}",
        out.stderr
    );
}
