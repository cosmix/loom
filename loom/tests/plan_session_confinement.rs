//! Real-sandbox confinement of an authoring planning session (section 12 acceptance).
//!
//! The authoring `settings.json` `write_session_sandbox` writes is translated for srt
//! (`plan_session_confinement/srt.rs`) and probed from the project root with a fake home: the
//! planning commands work, a raw write to code, `.git` and a locked plan is refused, the
//! credential, runtime and token files cannot be read, and the loopback and Unix-socket routes
//! to another local process are closed.
//!
//! Linux only, and skipped (with a printed reason) unless `bwrap`, `socat` and `srt` are on
//! PATH and a trivial `srt -c true` runs: the criteria runner and a session nest a sandbox that
//! cannot start srt. `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns a skip into a failure.
//!
//! The tests run `#[serial]`: each srt run starts its own proxies and bwrap.

#[path = "plan_session_confinement/fixture.rs"]
mod fixture;
#[path = "plan_session_confinement/srt.rs"]
mod srt;

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use fixture::{fixture, Fixture, LOCKED_PLAN, LOOM, SECRET, SESSION_ID, SUMMARY_ID, SYMBOL};
use serial_test::serial;
use srt::{quote, Confined, Probe};

/// The fixture and its srt translation, or `None` when this host cannot run srt.
fn confined(test: &str) -> Option<(Fixture, Confined)> {
    if srt::skip(test) {
        return None;
    }
    let fixture = fixture();
    let confined = Confined::new(&fixture);
    Some((fixture, confined))
}

/// `loom <args>` as a shell command line.
fn loom(args: &str) -> String {
    format!("{} {args}", quote(LOOM))
}

fn assert_ok(probe: &Probe, what: &str) {
    assert_eq!(probe.rc, Some(0), "{what} must succeed: {}", probe.diag());
}

/// The probed command exited non-zero and printed no secret.
fn assert_refused(probe: &Probe, what: &str) {
    let failed = probe.rc.is_some_and(|rc| rc != 0);
    assert!(failed, "{what} must be refused: {}", probe.diag());
    assert!(
        !probe.stdout().contains(SECRET),
        "{what} leaked a secret: {}",
        probe.diag()
    );
}

fn assert_unreadable(confined: &Confined, path: &Path) {
    let probe = confined.run(&format!("cat {}", quote(path)), &[]);
    assert_refused(&probe, &format!("reading {}", path.display()));
}

#[test]
#[serial]
fn knowledge_update_writes_the_knowledge_file() {
    let Some((fx, confined)) = confined("knowledge_update_writes_the_knowledge_file") else {
        return;
    };
    let probe = confined.run(
        &loom("knowledge update conventions 'Sandboxed-entry.'"),
        &[],
    );
    assert_ok(&probe, "loom knowledge update");
    let file = fx.root.join("doc/loom/knowledge/conventions.md");
    let text = fs::read_to_string(&file).unwrap_or_default();
    assert!(
        text.contains("Sandboxed-entry."),
        "{}: {text}",
        file.display()
    );
}

#[test]
#[serial]
fn summary_seal_writes_the_summary_with_the_session_environment() {
    let name = "summary_seal_writes_the_summary_with_the_session_environment";
    let Some((fx, confined)) = confined(name) else {
        return;
    };
    // Set as plan 7's launch sets them; the in-session check accepts the state because the
    // sandbox binds the runtime grants into the read-denied planning root from the host.
    let state = fx.runtime.dir.display().to_string();
    let envs = [
        ("LOOM_PLAN_SESSION", SESSION_ID),
        ("LOOM_PLAN_STATE", state.as_str()),
    ];
    let probe = confined.run(&loom(&format!("plan summary seal {SUMMARY_ID}")), &envs);
    // No fallback: when srt cannot re-allow the runtime grants beneath the planning-root deny,
    // the draft and the lock are unreachable and this is the failure to read.
    let carve_out = "the settings' allowRead carve-outs (drafts/, claims/, summaries.lock \
                     under the denied planning root) did not take effect in srt";
    assert_eq!(
        probe.rc,
        Some(0),
        "seal failed; {carve_out}: {}",
        probe.diag()
    );
    let summary = fx.root.join(format!("doc/loom/summaries/{SUMMARY_ID}.md"));
    assert!(summary.is_file(), "seal wrote no {}", summary.display());
}

/// srt covers the read-denied planning root with a private tmpfs per command, so a write there
/// succeeds inside the command; none may reach the host: no probe file appears, and the server
/// lock and the session's permission answers keep their content.
#[test]
#[serial]
fn writes_to_the_planning_root_and_runtime_never_reach_the_host() {
    let name = "writes_to_the_planning_root_and_runtime_never_reach_the_host";
    let Some((fx, confined)) = confined(name) else {
        return;
    };
    let answers = fx.session.answers().join("a.json");
    for dir in [&fx.runtime.planning_root, &fx.runtime.dir] {
        let target = dir.join("probe");
        let script = format!(
            "touch {}; echo x > {}; echo x > {}",
            quote(&target),
            quote(&fx.runtime.lock),
            quote(&answers)
        );
        let probe = confined.run(&script, &[]);
        assert!(
            probe.rc.is_some(),
            "the shell did not reach its last line: {}",
            probe.diag()
        );
        assert!(!target.exists(), "{} reached the host", target.display());
    }
    assert_eq!(
        fs::read_to_string(&fx.runtime.lock).unwrap(),
        SECRET,
        "the server lock was rewritten"
    );
    assert_eq!(
        fs::read_to_string(&answers).unwrap(),
        SECRET,
        "the session's permission answers were rewritten"
    );
}

#[test]
#[serial]
fn map_prints_an_outline_with_the_cache_write_denied() {
    let name = "map_prints_an_outline_with_the_cache_write_denied";
    let Some((_fx, confined)) = confined(name) else {
        return;
    };
    let probe = confined.run(&loom("map --outline loom/src/lib.rs"), &[]);
    assert_ok(&probe, "loom map --outline");
    assert!(
        probe.stdout().contains(SYMBOL),
        "no outline: {}",
        probe.diag()
    );
}

#[test]
#[serial]
fn knowledge_context_answers_a_query() {
    let Some((_fx, confined)) = confined("knowledge_context_answers_a_query") else {
        return;
    };
    let args = "knowledge context --query conventions --budget-tokens 500";
    assert_ok(&confined.run(&loom(args), &[]), "loom knowledge context");
}

#[test]
#[serial]
fn a_raw_write_to_code_is_refused() {
    let Some((fx, confined)) = confined("a_raw_write_to_code_is_refused") else {
        return;
    };
    let file = fx.root.join("loom/src/lib.rs");
    let before = fs::read(&file).unwrap();
    assert_refused(
        &confined.run("echo x > loom/src/lib.rs", &[]),
        "a write to code",
    );
    assert_eq!(fs::read(&file).unwrap(), before, "the code file changed");
}

#[test]
#[serial]
fn a_raw_write_to_git_is_refused() {
    let Some((fx, confined)) = confined("a_raw_write_to_git_is_refused") else {
        return;
    };
    assert_refused(&confined.run("echo x > .git/probe", &[]), "a write to .git");
    assert!(
        !fx.root.join(".git/probe").exists(),
        ".git/probe was created"
    );
}

#[test]
#[serial]
fn the_doc_tree_stays_writable() {
    let Some((fx, confined)) = confined("the_doc_tree_stays_writable") else {
        return;
    };
    let probe = confined.run("echo x > doc/product/new.md", &[]);
    assert_ok(&probe, "a write to doc/product/new.md");
    assert!(
        fx.root.join("doc/product/new.md").is_file(),
        "the write did not land"
    );
}

#[test]
#[serial]
fn a_locked_plan_is_write_denied() {
    let Some((fx, confined)) = confined("a_locked_plan_is_write_denied") else {
        return;
    };
    let file = fx.root.join(LOCKED_PLAN);
    let before = fs::read(&file).unwrap();
    let probe = confined.run(&format!("echo x > {LOCKED_PLAN}"), &[]);
    assert_refused(&probe, "a write to a running plan");
    assert_eq!(fs::read(&file).unwrap(), before, "the locked plan changed");
}

#[test]
#[serial]
fn the_runtime_lock_and_answers_are_unreadable() {
    let name = "the_runtime_lock_and_answers_are_unreadable";
    let Some((fx, confined)) = confined(name) else {
        return;
    };
    assert_unreadable(&confined, &fx.runtime.lock);
    assert_unreadable(&confined, &fx.session.answers().join("a.json"));
}

#[test]
#[serial]
fn a_credential_file_is_unreadable() {
    let Some((fx, confined)) = confined("a_credential_file_is_unreadable") else {
        return;
    };
    assert_unreadable(&confined, &fx.home.join(".ssh/id_rsa"));
}

#[test]
#[serial]
fn a_state_root_token_is_unreadable() {
    let Some((fx, confined)) = confined("a_state_root_token_is_unreadable") else {
        return;
    };
    assert_unreadable(&confined, &fx.root.join(".loom/work/admin.token"));
}

#[test]
#[serial]
fn a_loopback_tcp_listener_is_unreachable() {
    let name = "a_loopback_tcp_listener_is_unreachable";
    let Some((_fx, confined)) = confined(name) else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    TcpStream::connect(addr).expect("the listener is reachable outside the sandbox");
    let connect = format!("bash -c 'exec 3<>/dev/tcp/127.0.0.1/{}'", addr.port());
    assert_refused(&confined.run(&connect, &[]), "a loopback TCP connect");
}

#[test]
#[serial]
fn a_unix_socket_is_unreachable() {
    let Some((fx, confined)) = confined("a_unix_socket_is_unreachable") else {
        return;
    };
    let path = fx.base.join("sock");
    let _listener = UnixListener::bind(&path).unwrap();
    UnixStream::connect(&path).expect("the socket is reachable outside the sandbox");
    let connect = format!("socat - UNIX-CONNECT:{}", quote(&path));
    assert_refused(&confined.run(&connect, &[]), "a Unix-socket connect");
}
