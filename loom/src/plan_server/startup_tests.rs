//! Tests for startup terminal output.

use std::path::Path;

use super::lock::LockRecord;
use super::startup::{already_running_lines, startup_lines, tunnel_hint, Startup};
use super::wire::server::{MissingPrereq, PrereqKind};

#[test]
fn startup_lines_include_every_optional_line_in_order() {
    let problems = vec![
        MissingPrereq {
            what: PrereqKind::Tmux,
            message: "tmux missing".into(),
            fix: "Install tmux".into(),
        },
        MissingPrereq {
            what: PrereqKind::Claude,
            message: "Claude missing".into(),
            fix: "Install Claude".into(),
        },
    ];
    let startup = Startup {
        root: Path::new("/project\nname"),
        urls: vec![
            "http://127.0.0.1:7272/?token=abc".into(),
            "http://192.0.2.1:7272/?token=abc".into(),
        ],
        port: 7272,
        remote: true,
        ssh: true,
        hostname: "host\nname".into(),
        read_only: &problems,
    };
    assert_eq!(startup_lines(&startup), vec![
        "loom plan: /project name",
        "http://127.0.0.1:7272/?token=abc",
        "http://192.0.2.1:7272/?token=abc",
        "On your machine: ssh -L 7272:127.0.0.1:7272 host name",
        "warning: plain HTTP; anyone who can see the traffic can read the token, then read every document and write through sessions. An SSH tunnel is the safer way to share.",
        "read-only: tmux missing. Install tmux",
        "read-only: Claude missing. Install Claude",
        "Ctrl-C to stop; a second Ctrl-C exits at once",
    ]);
}

#[test]
fn startup_lines_omit_optional_lines() {
    let startup = Startup {
        root: Path::new("/project"),
        urls: vec![],
        port: 1234,
        remote: false,
        ssh: false,
        hostname: "host".into(),
        read_only: &[],
    };
    assert_eq!(
        startup_lines(&startup),
        vec![
            "loom plan: /project",
            "Ctrl-C to stop; a second Ctrl-C exits at once"
        ]
    );
    assert_eq!(tunnel_hint(1234, "host"), "ssh -L 1234:127.0.0.1:1234 host");
}

#[test]
fn already_running_lines_join_url_and_handle_incomplete_record() {
    let mut record = LockRecord {
        pid: 42,
        start_time: None,
        url: Some("http://localhost:7272".into()),
        token: Some("abc".into()),
    };
    let expected = vec![
        "loom plan is already running for this project (pid 42).",
        "Open http://localhost:7272/?token=abc",
    ];
    assert_eq!(already_running_lines(&record), expected);
    record.url = Some("http://localhost:7272/".into());
    assert_eq!(already_running_lines(&record), expected);
    record.url = None;
    assert_eq!(
        already_running_lines(&record),
        vec![
            "loom plan is already running for this project (pid 42).",
            "It is still starting; run loom plan again in a moment."
        ]
    );
    record.url = Some("http://localhost:7272".into());
    record.token = None;
    assert_eq!(
        already_running_lines(&record),
        vec![
            "loom plan is already running for this project (pid 42).",
            "It is still starting; run loom plan again in a moment."
        ]
    );
}
