//! Real-tmux e2e test for the viewer reconciler
//! ([`loom::orchestrator::terminal::tmux::reconcile_viewer`]).
//!
//! Every OTHER reconcile test is PURE (`src/orchestrator/terminal/tmux/reconcile/steps_tests.rs`
//! via `reconcile::steps::reconcile_steps`) and never touches a real tmux
//! server. This is the one place that drives the reconciler against an
//! actual viewer plus real inner sessions, proving the pure diff really
//! converges real panes: an add brings in a new session, a dead pane is
//! killed on the SAME pass a replacement is added, and the floor rule keeps
//! the last dead pane as a placeholder rather than emptying the window.
//!
//! Reuses the sandbox/skip machinery from `tmux_backend.rs` (see its module
//! doc for why `TMUX_TMPDIR` is redirected to an isolated directory and the
//! bind is probed before trusting the environment) rather than duplicating
//! it. `#[serial]` because, like every test in that file, this one mutates
//! process-global env state.

use loom::orchestrator::terminal::tmux::viewer::{
    pane_command, viewer_socket_name, OVERVIEW_SESSION,
};
use loom::orchestrator::terminal::tmux::{reconcile_viewer, socket_path_for};
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

use crate::tmux_backend::{
    skip_unless_tmux_can_bind, wait_until, TmuxServerGuard, TmuxTmpDirGuard,
};

/// `-F` format for `list-panes` — the same shape `reconcile.rs`'s
/// `PANE_FORMAT` parses in production.
const PANE_FORMAT: &str = "#{pane_id}\t#{pane_dead}\t#{pane_start_command}";

/// Mirrors `VIEWER_HARDENING` in `commands/attach/overview.rs` (private, so
/// reproduced by hand): brings the server up and makes a dying pane 0
/// survivable BEFORE it exists — see that constant's doc for why.
const HARDENING_ARGS: &[&str] = &[
    "start-server",
    ";",
    "set-option",
    "-g",
    "exit-empty",
    "off",
    ";",
    "set-option",
    "-g",
    "-w",
    "remain-on-exit",
    "on",
    ";",
    "set-option",
    "-g",
    "mouse",
    "off",
    ";",
    "set-option",
    "-g",
    "terminal-overrides[99]",
    "*:kmous@",
];

/// Runs one control command against `socket`, always with `TMUX` unset so
/// the test also passes when run from inside a tmux session (the same
/// reasoning as the `TMUX_TMPDIR` isolation in the module doc).
fn tmux(socket: &str, args: &[&str]) -> Output {
    let mut full: Vec<&str> = vec!["-L", socket];
    full.extend_from_slice(args);
    Command::new("tmux")
        .args(&full)
        .env_remove("TMUX")
        .output()
        .expect("tmux command should run")
}

/// Asserts a [`tmux`] call succeeded, folding stderr into the failure.
fn assert_ok(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Starts a real inner tmux server hosting one long-lived session, guarded
/// so it is torn down (best-effort, idempotent even if already dead) on
/// every exit path including a panic.
fn start_inner_server(socket: &str, session: &str) -> TmuxServerGuard {
    let output = tmux(
        socket,
        &[
            "new-session",
            "-d",
            "-s",
            session,
            "-x",
            "80",
            "-y",
            "24",
            "sleep",
            "300",
        ],
    );
    assert_ok(&output, &format!("starting inner tmux server '{socket}'"));
    TmuxServerGuard {
        socket_path: socket_path_for(socket),
    }
}

/// Brings up the viewer exactly as `loom attach` does, with its first pane
/// already attached into `(inner_socket, inner_session)`. Mirrors
/// `commands/attach/overview.rs::build_overview_argv`, which is private, so
/// this test reproduces the sequence by hand rather than importing it.
fn build_viewer(viewer_socket: &str, inner_socket: &str, inner_session: &str) {
    // (a) Best-effort teardown of a previous overview; there may be none.
    let _ = tmux(viewer_socket, &["kill-session", "-t", OVERVIEW_SESSION]);

    // (b) Bring the server up and make a dying pane 0 survivable BEFORE it
    // exists — see [`HARDENING_ARGS`]'s doc.
    assert_ok(&tmux(viewer_socket, HARDENING_ARGS), "viewer hardening");

    // (c) Create the viewer, pane 0 already attached into the inner server.
    let pane = pane_command(inner_socket, inner_session);
    let new_session = tmux(
        viewer_socket,
        &[
            "new-session",
            "-d",
            "-s",
            OVERVIEW_SESSION,
            "-x",
            "220",
            "-y",
            "50",
            "sh",
            "-c",
            pane.as_str(),
        ],
    );
    assert_ok(&new_session, "viewer new-session");

    // (d) Re-assert on the window itself — belt-and-braces re-assertion of
    // (b), see `REMAIN_ON_EXIT_FLAGS`'s doc in commands/attach/overview.rs.
    let remain_on_exit = tmux(
        viewer_socket,
        &[
            "set-option",
            "-w",
            "-t",
            OVERVIEW_SESSION,
            "remain-on-exit",
            "on",
        ],
    );
    assert_ok(&remain_on_exit, "viewer remain-on-exit re-assertion");
}

/// `(pane_id, dead, start_command)` for every pane in the viewer window, in
/// tmux's own pane order.
fn list_overview_panes(viewer_socket: &str) -> Vec<(String, bool, String)> {
    let output = tmux(
        viewer_socket,
        &["list-panes", "-t", OVERVIEW_SESSION, "-F", PANE_FORMAT],
    );
    assert_ok(&output, "viewer list-panes");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let id = fields.next()?.to_string();
            let dead = fields.next()? == "1";
            let start_command = fields.next().unwrap_or("").to_string();
            Some((id, dead, start_command))
        })
        .collect()
}

/// Whether pane 0 of the viewer currently reports dead.
fn pane0_is_dead(viewer_socket: &str) -> bool {
    list_overview_panes(viewer_socket)
        .first()
        .map(|(_, dead, _)| *dead)
        .unwrap_or(false)
}

/// Whether `panes` is exactly one pane matching `expect_dead`/
/// `expect_socket`. The `-L <expect_socket>` substring check pins the real
/// tmux 3.6a `#{pane_start_command}` rendering `parse_pane_socket` depends
/// on. Takes an already-fetched pane list so [`assert_single_pane`] fetches
/// once, not twice, and [`wait_for_single_pane`] can re-fetch per poll.
fn panes_match(panes: &[(String, bool, String)], expect_dead: bool, expect_socket: &str) -> bool {
    panes.len() == 1
        && panes[0].1 == expect_dead
        && panes[0].2.contains(&format!("-L {expect_socket}"))
}

/// Asserts the viewer window holds exactly one pane right now, matching
/// `expect_dead`/`expect_socket`.
fn assert_single_pane(viewer_socket: &str, expect_dead: bool, expect_socket: &str) {
    let panes = list_overview_panes(viewer_socket);
    assert!(
        panes_match(&panes, expect_dead, expect_socket),
        "expected exactly one pane (dead={expect_dead}) targeting '{expect_socket}': {panes:?}"
    );
}

/// Polls up to 5s — the window it takes tmux to notice an inner server died
/// or a reconcile pass to converge.
fn wait_for_single_pane(viewer_socket: &str, expect_dead: bool, expect_socket: &str) {
    assert!(
        wait_until(
            || panes_match(
                &list_overview_panes(viewer_socket),
                expect_dead,
                expect_socket
            ),
            Duration::from_secs(5)
        ),
        "timed out waiting for one pane (dead={expect_dead}) targeting '{expect_socket}'"
    );
}

/// The id of the sole pane in the viewer window right now.
fn pane0_id(viewer_socket: &str) -> String {
    list_overview_panes(viewer_socket)
        .first()
        .expect("the viewer should have at least one pane")
        .0
        .clone()
}

/// Asserts the SAME pane (by id) still exists, dead. Deliberately stronger
/// than "still 1 pane": that alone also holds if the reconciler did
/// NOTHING, so the caller captures [`pane0_id`] before the call under test
/// and this checks the survivor is literally that pane, not a coincidental
/// look-alike or a kill-then-recreate.
fn assert_pane_survived_dead(viewer_socket: &str, expect_id: &str) {
    let panes = list_overview_panes(viewer_socket);
    assert_eq!(
        panes.len(),
        1,
        "expected exactly one surviving pane: {panes:?}"
    );
    assert_eq!(
        panes[0].0, expect_id,
        "the SAME dead pane must survive the floor rule, not a kill+recreate: {panes:?}"
    );
    assert!(
        panes[0].1,
        "the surviving pane must still be dead: {panes:?}"
    );
}

/// Kills the inner server at `inner_socket` and waits (≤5s) for the
/// viewer's pane 0 — the only pane attached to it in this test — to go dead.
fn kill_inner_and_wait_dead(viewer_socket: &str, inner_socket: &str) {
    let _ = tmux(inner_socket, &["kill-server"]);
    assert!(
        wait_until(|| pane0_is_dead(viewer_socket), Duration::from_secs(5)),
        "pane 0 should go dead once inner server '{inner_socket}' is killed"
    );
}

/// `reconcile_viewer` desiring a single `(socket, session)` pair, panicking
/// with `context` on failure.
fn reconcile_one(work_dir: &Path, socket: &str, session: &str, context: &str) {
    let desired = [(socket.to_string(), session.to_string())];
    reconcile_viewer(work_dir, &desired).unwrap_or_else(|err| panic!("{context}: {err}"));
}

/// `reconcile_viewer` desiring nothing, panicking with `context` on failure.
fn reconcile_empty(work_dir: &Path, context: &str) {
    reconcile_viewer(work_dir, &[]).unwrap_or_else(|err| panic!("{context}: {err}"));
}

/// Steps 1-3: an isolated `.work` dir, inner server A, and the viewer built
/// on it, pane 0 already attached into A. Returns the guards (kept alive by
/// the caller for the rest of the test) plus the derived paths.
fn setup_viewer_over_server_a(
    socket_a: &str,
    session_a: &str,
) -> (TempDir, TmuxServerGuard, TmuxServerGuard, PathBuf, String) {
    // `work_dir` only needs to exist and be canonicalizable —
    // `reconcile_viewer` uses it solely to derive the viewer socket name.
    let repo_dir = TempDir::new().expect("tempdir creation should succeed");
    let work_dir = repo_dir.path().join(".work");
    std::fs::create_dir_all(&work_dir).expect(".work dir should be creatable");
    let viewer_socket = viewer_socket_name(&work_dir);

    let server_a = start_inner_server(socket_a, session_a);
    // Guard constructed BEFORE `build_viewer` starts the server (its first
    // hardening step is `start-server`): a panic mid-build must still tear
    // the viewer server down, not strand it once `TmuxTmpDirGuard::drop`
    // removes the socket dir out from under an unguarded server.
    let viewer_guard = TmuxServerGuard {
        socket_path: socket_path_for(&viewer_socket),
    };
    build_viewer(&viewer_socket, socket_a, session_a);

    (repo_dir, server_a, viewer_guard, work_dir, viewer_socket)
}

#[test]
#[serial]
fn reconcile_viewer_adds_new_sessions_and_kills_dead_panes_on_real_tmux() {
    let tmux_tmpdir = TmuxTmpDirGuard::new();
    if skip_unless_tmux_can_bind(
        tmux_tmpdir.dir(),
        "reconcile_viewer_adds_new_sessions_and_kills_dead_panes_on_real_tmux",
    ) {
        return;
    }

    // 1-3. An isolated `.work` dir, inner server A, and the viewer built on
    // it exactly as `loom attach` would, pane 0 already attached into A.
    let socket_a = "loom-session-aaaa";
    let session_a = "key-a";
    let (_repo_dir, _server_a, _viewer_guard, work_dir, viewer_socket) =
        setup_viewer_over_server_a(socket_a, session_a);

    // 4. Exactly one pane, alive, attached into A.
    assert_single_pane(&viewer_socket, false, socket_a);

    // 5. A true no-op: reality already matches desired.
    reconcile_one(&work_dir, socket_a, session_a, "no-op reconcile");
    assert_single_pane(&viewer_socket, false, socket_a);

    // 6. Kill server A; its pane goes dead once tmux reaps the attach client.
    kill_inner_and_wait_dead(&viewer_socket, socket_a);

    // 7. Server B comes up; reconciling against B must add B's pane AND
    // kill dead A's pane in the SAME pass.
    let socket_b = "loom-session-bbbb";
    let session_b = "key-b";
    let _server_b = start_inner_server(socket_b, session_b);
    reconcile_one(&work_dir, socket_b, session_b, "add B, kill dead A");
    wait_for_single_pane(&viewer_socket, false, socket_b);

    // 8. Kill B too; the floor rule must keep the single dead pane rather
    // than emptying the window. Capture its id first: "still 1 pane" alone
    // would also hold if the reconciler did nothing, so the survivor must
    // be proven to be THIS pane.
    kill_inner_and_wait_dead(&viewer_socket, socket_b);
    let dead_pane_id = pane0_id(&viewer_socket);
    reconcile_empty(&work_dir, "reconcile with nothing desired");
    assert_pane_survived_dead(&viewer_socket, &dead_pane_id);
}
