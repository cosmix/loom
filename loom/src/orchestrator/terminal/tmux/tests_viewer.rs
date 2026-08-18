//! Pins the identity half of `viewer.rs` — socket/session naming, the pane
//! command builder, and `attachable_panes`'s selection rule. Split from
//! `tests_discovery.rs` only to keep both files under the 400-line ceiling
//! (CLAUDE.md Rule 17); the discovery half (`live_tmux_sessions` and its
//! filters) lives there.

use super::*;
use chrono::{DateTime, Utc};
use std::process::Command;
use tempfile::TempDir;

/// An in-memory session for the selection tests, which never touch the
/// filesystem or tmux. Used directly by this file's `attachable_panes`
/// tests; also shared with `commands/attach/tests.rs`, whose
/// `newest_for_stage_*` tests need the same fixture.
pub(crate) fn stub_session(id: &str, stage_id: &str, created_at: DateTime<Utc>) -> Session {
    let mut session = Session::new();
    session.id = id.to_string();
    session.assign_to_stage(stage_id.to_string());
    session.created_at = created_at;
    session
}

#[test]
fn attachable_panes_drops_sessions_whose_server_is_not_ready() {
    // The gap this closes: discovery admits `Spawning` sessions and judges
    // liveness by PID alone (deliberately — see `endpoint_ready`), so a
    // session can be live by every existing filter while its tmux server is
    // not yet, or no longer, accepting clients. A pane pointed at one exits
    // on contact.
    let now = Utc::now();
    let sessions = vec![
        stub_session("session-ready", "stage-ready", now),
        stub_session("session-not-ready", "stage-not-ready", now),
    ];

    let panes = attachable_panes(&sessions, |session, _| session.id == "session-ready");

    assert_eq!(
        panes,
        vec![(
            "loom-session-ready".to_string(),
            "loom-stage-ready".to_string()
        )],
        "only the attachable session may become a pane, wired to its own \
         socket and tracking key"
    );
}

#[test]
fn attachable_panes_preserves_discovery_order() {
    // Pane order is the overview's one determinism guarantee (discovery sorts
    // by `(created_at, id)`); a filter that reorders would silently undo it.
    let now = Utc::now();
    let sessions = vec![
        stub_session("session-a", "stage-a", now - chrono::Duration::seconds(20)),
        stub_session("session-b", "stage-b", now - chrono::Duration::seconds(10)),
        stub_session("session-c", "stage-c", now),
    ];

    let panes = attachable_panes(&sessions, |_, _| true);
    let stages: Vec<&str> = panes.iter().map(|(_, key)| key.as_str()).collect();

    assert_eq!(stages, vec!["loom-stage-a", "loom-stage-b", "loom-stage-c"]);
}

#[test]
fn viewer_socket_name_differs_between_repos() {
    let temp_a = TempDir::new().unwrap();
    let temp_b = TempDir::new().unwrap();
    let work_a = temp_a.path().join(".work");
    let work_b = temp_b.path().join(".work");
    std::fs::create_dir_all(&work_a).unwrap();
    std::fs::create_dir_all(&work_b).unwrap();

    let socket_a1 = viewer_socket_name(&work_a);
    let socket_a2 = viewer_socket_name(&work_a);
    let socket_b = viewer_socket_name(&work_b);

    assert_eq!(socket_a1, socket_a2, "same root is stable across calls");
    assert_ne!(socket_a1, socket_b, "different roots must differ");
}

#[test]
fn viewer_socket_name_resolves_through_a_symlinked_dot_work() {
    // `.work` is a SYMLINK to the main repo's `.work` inside a worktree (see
    // `viewer_socket_name`'s doc comment) — that is the whole reason
    // `canonicalize()` is called at all. Simulate exactly that: a
    // "worktree" temp dir whose `.work` is a symlink into a separate
    // "main repo" temp dir's real `.work`, and assert both resolve to the
    // SAME viewer socket — proving the worktree case lands on the one
    // socket every worktree of a repo must share.
    let main_repo = TempDir::new().unwrap();
    let real_work_dir = main_repo.path().join(".work");
    std::fs::create_dir_all(&real_work_dir).unwrap();

    let worktree = TempDir::new().unwrap();
    let symlinked_work_dir = worktree.path().join(".work");
    std::os::unix::fs::symlink(&real_work_dir, &symlinked_work_dir).unwrap();

    let socket_via_symlink = viewer_socket_name(&symlinked_work_dir);
    let socket_via_real_path = viewer_socket_name(&real_work_dir);

    assert_eq!(
        socket_via_symlink, socket_via_real_path,
        "a worktree's symlinked .work must resolve to the same viewer socket as the main repo's real .work"
    );
}

#[test]
fn pane_command_targets_the_sessions_own_socket() {
    let cmd = pane_command("loom-session-abcd1234-1700000000", "loom-my-stage");
    assert_eq!(
        cmd,
        "unset TMUX; exec tmux -L loom-session-abcd1234-1700000000 attach-session -t loom-my-stage"
    );
}

/// A PATH whose first entry holds a no-op `tmux` stub, so a shell command
/// that `exec`s `tmux` off this PATH can never reach a real tmux and can
/// never leave a socket behind in the operator's own socket dir. Returns
/// the backing `TempDir` too — the caller must keep it alive at least until
/// the shell process using this PATH has exited, or the stub is deleted out
/// from under it.
///
/// `pane_command_neutralises_hostile_session_ids` (below) actually runs
/// `exec tmux -L <socket> attach-session ...` through a real shell, and a
/// REAL `tmux` on PATH resolves `-L` against the OPERATOR'S OWN socket dir
/// (`loom_socket_dir()` in `socket.rs` — `$TMUX_TMPDIR` else `/tmp`, joined
/// with `tmux-<uid>`), so an unguarded run there previously left a stray
/// socket behind in that directory. Prepending a stub-only directory
/// shadows `tmux` so `exec tmux ...` finds the stub and exits immediately
/// instead of touching a real socket.
///
/// This does NOT weaken that test's injection check: only `tmux` is
/// shadowed, the rest of PATH stays intact behind the stub, so `touch`/`id`
/// (what the hostile `$(...)`/backtick payload actually invokes) still
/// resolve normally. The command substitution that creates the probe file
/// fires during word expansion of the `exec tmux ...` command, before
/// `exec` ever runs, so it is completely unaffected by what `tmux` resolves
/// to (or whether it resolves at all) — if `escape_arg` ever stopped
/// escaping, the probe would still be created exactly as before this stub
/// existed.
fn stub_tmux_path() -> (TempDir, String) {
    let stub_bin = TempDir::new().unwrap();
    let fake_tmux = stub_bin.path().join("tmux");
    std::fs::write(&fake_tmux, "#!/bin/sh\nexit 0\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}",
        stub_bin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (stub_bin, path)
}

/// Runs `cmd` through a real shell exactly as `loom attach` would, with
/// `tmux` shadowed by [`stub_tmux_path`]'s no-op stub so the run can never
/// reach a real tmux server or leave a real socket behind. `.output()`
/// blocks until the child (and anything it `exec`s) has exited, so the
/// stub's `TempDir` is safe to drop the moment this returns.
fn run_shell_with_stub_tmux(cmd: &str) -> std::process::Output {
    let (_stub_bin, no_real_tmux_path) = stub_tmux_path();
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .env("PATH", no_real_tmux_path)
        .output()
        .unwrap()
}

#[test]
fn pane_command_neutralises_hostile_session_ids() {
    // `socket_name()`/`tracking_key` are read VERBATIM out of
    // `.work/sessions/*.md` with no validation on the read path (only
    // CLI args go through `validate_id`), and `.work/` is writable by
    // sandboxed stage agents, while `loom attach` runs in the operator's
    // UNSANDBOXED shell. A single quote, `;`, `$`, a backtick, and a
    // space must all be neutralised.
    //
    // The injection vector below is `$(touch ...)` COMMAND SUBSTITUTION,
    // not the trailing `; touch ...` alone: `pane_command`'s template is
    // `... exec tmux -L <here> attach-session -t <here>`, and `exec`, if it
    // finds `tmux` on PATH, REPLACES the shell process outright — verified
    // by hand with `sh -c 'exec tmux -L x; touch probe'`, which never
    // creates `probe` even fully unescaped, because `exec` never returns to
    // run the trailing `;`-separated command. Command substitution has no
    // such gap: it runs during WORD EXPANSION of the `exec tmux -L <here>`
    // command itself, before `exec` ever executes, so it fires regardless
    // of whether the subsequent `tmux` invocation succeeds, fails, or
    // replaces the process. The trailing `` `id` ``/quote/`#` noise stays
    // to additionally fuzz backtick, quote, and comment handling.
    let temp = TempDir::new().unwrap();
    let probe = temp.path().join("pwned");
    let hostile = format!("loom-x$(touch {}); `id`; 'literal'; #", probe.display());

    // Round-trip the ESCAPED token back out through a real POSIX shell,
    // exactly like `shell_round_trip` in
    // `orchestrator/terminal/tmux/tests.rs`: `printf '%s' <escaped>`
    // proves the whole hostile value decodes back to ONE literal word
    // instead of being split into a `touch` command and a `#` comment.
    let escaped = escape_arg(&hostile);
    let printf_output = Command::new("sh")
        .arg("-c")
        .arg(format!("printf '%s' {escaped}"))
        .output()
        .expect("sh should be available to round-trip the escaped argument");
    assert!(printf_output.status.success());
    assert_eq!(
        String::from_utf8(printf_output.stdout).unwrap(),
        hostile,
        "escaping must decode back to the exact original session id"
    );

    // Executing the FULL pane command must never run the injected
    // `touch`: whether or not the subsequent `exec tmux` itself
    // succeeds is irrelevant to the injection check, so this holds
    // regardless of whether tmux is installed in the test environment.
    // `run_shell_with_stub_tmux` keeps that `exec tmux` from ever reaching
    // a real tmux server — see its doc comment for why that doesn't weaken
    // this check.
    let cmd = pane_command("loom-test-neutralises-hostile-ids", &hostile);
    let _ = run_shell_with_stub_tmux(&cmd);
    assert!(
        !probe.exists(),
        "a hostile session id must never execute an injected shell command"
    );

    // `pane_command` takes TWO interpolated arguments and the block above
    // only fuzzed `tmux_session`. `session_socket` is
    // `format!("loom-{}", session.id)` (`orchestrator/terminal/tmux/mod.rs:41-43`)
    // where `session.id` is read off the exact same unvalidated
    // `.work/sessions/*.md` read path — equally attacker-controlled, and
    // would slip through untested if only `tmux_session` were ever fuzzed.
    let cmd_socket = pane_command(&hostile, "loom-test-neutralises-hostile-ids");
    let _ = run_shell_with_stub_tmux(&cmd_socket);
    assert!(
        !probe.exists(),
        "a hostile session_socket must never execute an injected shell command"
    );
}

#[test]
fn attachable_panes_skips_a_session_whose_id_needs_shell_quoting() {
    // `session.id` feeds `socket_name()`, which `escape_arg` will
    // shell-quote once it contains a space (`'loom-a b'`). The reconciler
    // then tokenises the pane's recorded start command on whitespace to
    // recover the socket, gets back the truncated `loom-a`, and never
    // matches it to this session — so it concludes the session has no
    // pane and emits an ADD every tick, forever, until `split-window`
    // fails for lack of space. Excluding the session from the pane list
    // in the first place is what stops that loop before it starts.
    let now = Utc::now();
    let sessions = vec![
        stub_session("session-normal", "stage-normal", now),
        stub_session(
            "session with space",
            "stage-quoted",
            now + chrono::Duration::seconds(1),
        ),
    ];

    let panes = attachable_panes(&sessions, |_, _| true);

    assert_eq!(
        panes,
        vec![(
            "loom-session-normal".to_string(),
            "loom-stage-normal".to_string()
        )],
        "only the session with a plain id may become a pane"
    );
}

#[test]
fn attachable_panes_skips_a_session_whose_tracking_key_needs_shell_quoting() {
    // The sibling of the test above, fuzzing the OTHER interpolated slot:
    // `pane_command_neutralises_hostile_session_ids` calls out in its own
    // comment that `pane_command` takes two arguments and a test that only
    // fuzzes one of them leaves the other's round trip unverified. Here
    // `tracking_key` (derived from a stage id containing a space) is the
    // one that would make `escape_arg` shell-quote the tmux session name,
    // truncating under the reconciler's whitespace tokeniser exactly as
    // the id case does.
    let now = Utc::now();
    let sessions = vec![
        stub_session("session-normal", "stage-normal", now),
        stub_session(
            "session-quoted",
            "stage with space",
            now + chrono::Duration::seconds(1),
        ),
    ];

    let panes = attachable_panes(&sessions, |_, _| true);

    assert_eq!(
        panes,
        vec![(
            "loom-session-normal".to_string(),
            "loom-stage-normal".to_string()
        )],
        "only the session with a plain tracking key may become a pane"
    );
}

#[test]
fn is_plain_identifier_accepts_the_ids_loom_actually_generates() {
    // Positive control: without this test, a guard that rejected EVERY
    // value (not just the hostile ones) would still pass the two tests
    // above, since both only assert that the OTHER, well-formed session
    // survives.
    assert!(is_plain_identifier("loom-session-abc12345-1754900000"));
    assert!(is_plain_identifier("loom-merge-my-stage_1"));

    assert!(!is_plain_identifier("has space"));
    assert!(!is_plain_identifier("has\nnewline"));
    assert!(!is_plain_identifier("has\ttab"));
    assert!(!is_plain_identifier("has'quote"));
    assert!(!is_plain_identifier("has$dollar"));
    assert!(!is_plain_identifier(""));
}
