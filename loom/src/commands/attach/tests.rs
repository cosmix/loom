use super::*;
use chrono::{DateTime, Utc};
use tempfile::TempDir;

fn panes(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("loom-session-{i}"), format!("loom-stage-{i}")))
        .collect()
}

#[test]
fn build_overview_argv_emits_one_pane_per_session() {
    let steps = build_overview_argv("loom-view-deadbeef", &panes(3));
    let new_session_count = steps
        .iter()
        .filter(|s| s.get(2).map(String::as_str) == Some("new-session"))
        .count();
    let split_window_count = steps
        .iter()
        .filter(|s| s.get(2).map(String::as_str) == Some("split-window"))
        .count();
    assert_eq!(new_session_count, 1, "exactly one new-session step");
    assert_eq!(split_window_count, 2, "N-1 split-window steps for N=3");
}

#[test]
fn build_overview_argv_applies_tiled_layout_after_every_split() {
    let steps = build_overview_argv("loom-view-deadbeef", &panes(4));

    // remain-on-exit is a WINDOW option (see REMAIN_ON_EXIT_FLAGS docs):
    // it must be set right after new-session, before the first split, so
    // it protects panes created during the rest of the build too.
    let new_session_idx = steps
        .iter()
        .position(|s| s.get(2).map(String::as_str) == Some("new-session"))
        .expect("new-session step present");
    let remain_idx = steps
        .iter()
        .position(|s| s.get(2).map(String::as_str) == Some("set-option"))
        .expect("set-option remain-on-exit step present");
    assert_eq!(
        remain_idx,
        new_session_idx + 1,
        "remain-on-exit must be set immediately after new-session, before any split"
    );

    // Every split-window halves the pane it just split off (see
    // build_overview_argv's docs), so a select-layout must immediately
    // follow EVERY split-window, not just appear once at the end.
    let split_positions: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.get(2).map(String::as_str) == Some("split-window"))
        .map(|(idx, _)| idx)
        .collect();
    assert_eq!(split_positions.len(), 3, "N-1 split-window steps for N=4");

    for idx in &split_positions {
        let next = steps
            .get(idx + 1)
            .expect("a step must immediately follow every split-window");
        assert_eq!(
            next.get(2).map(String::as_str),
            Some("select-layout"),
            "select-layout must immediately follow every split-window"
        );
    }

    let select_layout_count = steps
        .iter()
        .filter(|s| s.get(2).map(String::as_str) == Some("select-layout"))
        .count();
    assert_eq!(
        select_layout_count,
        split_positions.len(),
        "exactly one select-layout per split, and no trailing one"
    );
}

#[test]
fn build_overview_argv_wires_each_pane_to_its_own_session() {
    // The tests above assert only pane/step COUNTS and formatting, so none
    // of them can see WHICH session ends up in which pane. That misses two
    // real regressions that leave every count/format assertion intact:
    //
    // (1) `attach.rs`'s split loop passing `Some(&panes[0])` instead of
    //     `Some(pane)` — the overview would tile session 0 into every pane
    //     instead of one pane per session.
    // (2) An off-by-one that shifts the split loop's window (e.g. splitting
    //     over `panes[0..panes.len() - 1]` instead of `panes[1..]`) — the
    //     step counts stay identical (1 new-session + N-1 split-window), but
    //     the LAST live session is silently dropped and an earlier one is
    //     duplicated instead.
    //
    // Asserting the exact ORDERED pane-command correspondence below catches
    // both; count-only assertions do not.
    let test_panes = panes(5);
    let steps = build_overview_argv("loom-view-deadbeef", &test_panes);

    let actual: Vec<&str> = steps
        .iter()
        .filter(|s| {
            matches!(
                s.get(2).map(String::as_str),
                Some("new-session") | Some("split-window")
            )
        })
        .map(|s| {
            s.last()
                .expect("pane step must carry a trailing pane command")
                .as_str()
        })
        .collect();

    let expected: Vec<String> = test_panes
        .iter()
        .map(|(socket, session)| pane_command(socket, session))
        .collect();

    assert_eq!(
        actual,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "pane i must attach to panes[i]'s own session, in order"
    );
}

#[test]
fn build_overview_argv_targets_the_per_repo_viewer_socket() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let socket = viewer_socket_name(&work_dir);

    let steps = build_overview_argv(&socket, &panes(2));
    for argv in &steps {
        assert_eq!(&argv[0], "-L");
        assert_eq!(&argv[1], &socket);
    }
    assert!(socket.starts_with("loom-view-"));
    let suffix = &socket["loom-view-".len()..];
    assert_eq!(suffix.len(), 8);
    assert!(suffix
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn build_overview_argv_every_pane_command_clears_tmux() {
    // Asserting "TMUX is absent from the argv" would be vacuous: `$TMUX`
    // is an environment property tmux sets for the pane's process, not an
    // argv token — so we must instead assert on the emitted pane command
    // STRING containing the `unset`.
    let steps = build_overview_argv("loom-view-deadbeef", &panes(3));
    for argv in &steps {
        let verb = argv.get(2).map(String::as_str);
        if matches!(verb, Some("new-session") | Some("split-window")) {
            // The pane command is now the LAST of THREE trailing argv
            // words ("sh", "-c", <command>), not appended bare: tmux
            // must run it under a guaranteed POSIX sh, never its own
            // `default-shell` (see build_overview_argv's docs).
            let len = argv.len();
            assert!(len >= 3, "pane step must carry sh -c <command>: {argv:?}");
            assert_eq!(
                argv[len - 3],
                "sh",
                "pane command must run under a guaranteed POSIX sh: {argv:?}"
            );
            assert_eq!(argv[len - 2], "-c", "sh must be invoked with -c: {argv:?}");
            let last = argv.last().expect("pane command is the last argv element");
            assert!(
                last.starts_with("unset TMUX; exec tmux -L "),
                "pane command must clear TMUX first: {last}"
            );
            assert!(
                last.contains("attach-session -t "),
                "pane command must attach: {last}"
            );
        }
    }
}

#[test]
fn build_overview_argv_is_empty_without_sessions() {
    let steps = build_overview_argv("loom-view-deadbeef", &[]);
    assert!(steps.is_empty());
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
    let cmd = pane_command("loom-test-neutralises-hostile-ids", &hostile);
    let _ = Command::new("sh").arg("-c").arg(&cmd).output().unwrap();
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
    let _ = Command::new("sh")
        .arg("-c")
        .arg(&cmd_socket)
        .output()
        .unwrap();
    assert!(
        !probe.exists(),
        "a hostile session_socket must never execute an injected shell command"
    );
}

/// Fields needed to render a fixture session file, grouped into a struct
/// so [`write_test_session`] stays under clippy's argument-count limit.
struct SessionFixture<'a> {
    id: &'a str,
    stage_id: &'a str,
    pid: u32,
    status: SessionStatus,
    backend: SessionBackendKind,
    created_at: DateTime<Utc>,
    tracking_key: &'a str,
}

/// Render a session file at `<work_dir>/sessions/<id>.md` via the SAME
/// serializer real sessions are persisted with
/// (`session_files::session_to_markdown`), so the fixture matches the
/// real on-disk format exactly instead of a hand-typed approximation.
fn write_test_session(work_dir: &Path, fixture: SessionFixture) {
    let mut session = Session::new();
    session.id = fixture.id.to_string();
    session.stage_id = Some(fixture.stage_id.to_string());
    session.pid = Some(fixture.pid);
    session.status = fixture.status;
    session.backend = fixture.backend;
    session.created_at = fixture.created_at;
    session.tracking_key = fixture.tracking_key.to_string();

    let sessions_dir = work_dir.join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::write(
        sessions_dir.join(format!("{}.md", fixture.id)),
        crate::fs::session_files::session_to_markdown(&session),
    )
    .unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(work_dir, &session, fixture.pid)
        .unwrap();
}

/// An in-memory session for the selection tests, which never
/// touch the filesystem or tmux.
fn stub_session(id: &str, stage_id: &str, created_at: DateTime<Utc>) -> Session {
    let mut session = Session::new();
    session.id = id.to_string();
    session.assign_to_stage(stage_id.to_string());
    session.created_at = created_at;
    session
}

#[test]
fn live_tmux_sessions_skips_native_backend_sessions() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    write_test_session(
        &work_dir,
        SessionFixture {
            id: "session-native",
            stage_id: "stage-a",
            pid: std::process::id(),
            status: SessionStatus::Running,
            backend: SessionBackendKind::Native,
            created_at: Utc::now(),
            tracking_key: "loom-stage-a",
        },
    );

    let sessions = live_tmux_sessions(&work_dir).unwrap();
    assert!(
        sessions.is_empty(),
        "a native-backend session must never be treated as tmux-hosted, even with a live pid"
    );
}

#[test]
fn live_tmux_sessions_skips_unparseable_session_files() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    std::fs::write(
        work_dir.join("sessions").join("session-garbage.md"),
        "this is not valid session frontmatter",
    )
    .unwrap();
    write_test_session(
        &work_dir,
        SessionFixture {
            id: "session-live",
            stage_id: "stage-b",
            pid: std::process::id(),
            status: SessionStatus::Running,
            backend: SessionBackendKind::Tmux,
            created_at: Utc::now(),
            tracking_key: "loom-stage-b",
        },
    );

    let sessions = live_tmux_sessions(&work_dir).unwrap();
    assert_eq!(
        sessions.len(),
        1,
        "one bad session file must never fail the whole discovery call"
    );
    assert_eq!(sessions[0].id, "session-live");
}

#[test]
fn live_tmux_sessions_skips_dead_sessions_but_keeps_live_ones() {
    // A fixture with ONLY a dead pid can pass `is_empty()` for the wrong
    // reason (e.g. discovery returning nothing at all). Writing a genuinely
    // live fixture in the SAME call, and asserting exactly it survives BY
    // ID, rules that out: the dead one must be filtered and the live one
    // must not be collateral damage.
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    write_test_session(
        &work_dir,
        SessionFixture {
            id: "session-dead",
            stage_id: "stage-c",
            pid: 999_999,
            status: SessionStatus::Running,
            backend: SessionBackendKind::Tmux,
            created_at: Utc::now(),
            tracking_key: "loom-stage-c",
        },
    );
    write_test_session(
        &work_dir,
        SessionFixture {
            id: "session-live",
            stage_id: "stage-d",
            pid: std::process::id(),
            status: SessionStatus::Running,
            backend: SessionBackendKind::Tmux,
            created_at: Utc::now(),
            tracking_key: "loom-stage-d",
        },
    );

    let sessions = live_tmux_sessions(&work_dir).unwrap();
    assert_eq!(
        sessions.len(),
        1,
        "exactly the live session must survive; the dead one must be filtered out"
    );
    assert_eq!(
        sessions[0].id, "session-live",
        "the surviving session must be the live one, not the dead one"
    );
}

#[test]
fn live_tmux_sessions_filters_by_status_not_just_pid() {
    // Every other `live_tmux_sessions` test above uses
    // `SessionStatus::Running`, so the
    // `matches!(session.status, Running | Spawning)` guard
    // (`attach/mod.rs:126-131`) is otherwise exercised by nothing: deleting
    // it breaks no test here, yet the real consequence is `loom attach`
    // offering a pane into a Completed/Crashed session whose pid has since
    // been recycled by an unrelated process. Pin both halves of the guard:
    // Completed must be excluded even with a live pid, Spawning must be
    // admitted.
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    write_test_session(
        &work_dir,
        SessionFixture {
            id: "session-completed",
            stage_id: "stage-done",
            pid: std::process::id(),
            status: SessionStatus::Completed,
            backend: SessionBackendKind::Tmux,
            created_at: Utc::now(),
            tracking_key: "loom-stage-done",
        },
    );
    write_test_session(
        &work_dir,
        SessionFixture {
            id: "session-spawning",
            stage_id: "stage-spawn",
            pid: std::process::id(),
            status: SessionStatus::Spawning,
            backend: SessionBackendKind::Tmux,
            created_at: Utc::now(),
            tracking_key: "loom-stage-spawn",
        },
    );

    let sessions = live_tmux_sessions(&work_dir).unwrap();
    assert_eq!(
        sessions.len(),
        1,
        "a Completed session must be filtered even with a live pid; a Spawning one must be admitted"
    );
    assert_eq!(
        sessions[0].id, "session-spawning",
        "Spawning must be treated as live, and Completed must not"
    );
}

/// Write a fixture session whose ON-DISK FILENAME is independent of the
/// `id` embedded in its frontmatter, so a test can pin filesystem-traversal
/// order against LOGICAL id order without relying on any particular
/// `read_dir` behaviour. [`write_test_session`] cannot do this — it always
/// names the file after `fixture.id` — which is exactly why the plain
/// version is insufficient for [`live_tmux_sessions_orders_by_created_at_then_id`].
fn write_test_session_with_filename(work_dir: &Path, filename: &str, fixture: SessionFixture) {
    let mut session = Session::new();
    session.id = fixture.id.to_string();
    session.stage_id = Some(fixture.stage_id.to_string());
    session.pid = Some(fixture.pid);
    session.status = fixture.status;
    session.backend = fixture.backend;
    session.created_at = fixture.created_at;
    session.tracking_key = fixture.tracking_key.to_string();

    let sessions_dir = work_dir.join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::write(
        sessions_dir.join(filename),
        crate::fs::session_files::session_to_markdown(&session),
    )
    .unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(work_dir, &session, fixture.pid)
        .unwrap();
}

#[test]
fn live_tmux_sessions_orders_by_created_at_then_id() {
    // `newest_for_stage_picks_the_newest_by_created_at` (below) hand-orders
    // its input vec, so it never exercises the `sort_by` at
    // `attach/mod.rs:106` — the actual source of pane-order determinism.
    // Deleting that sort breaks no other test in this file, and pane order
    // would silently become dependent on `read_dir`'s unspecified
    // filesystem order.
    //
    // A naive "write session-z's file before session-a's" fixture is NOT
    // enough to pin this everywhere: verified empirically here, macOS/APFS
    // returns `read_dir` entries already sorted BY FILENAME regardless of
    // write order, so whenever a fixture's filename happens to equal its
    // logical id (as [`write_test_session`] always does), alphabetical
    // filename order and ascending-id order coincide and a MISSING sort
    // goes undetected. To make this filesystem-order-independent, the
    // FILENAME and the embedded `id` are deliberately decoupled: the file
    // that is both written FIRST and sorts FIRST alphabetically
    // (`aaa-...`) carries the id that must sort LAST ("session-z-tied"),
    // and vice versa — so "trust whatever order read_dir/insertion gives"
    // and "trust (created_at, id) order" disagree no matter which of those
    // two common traversal strategies a filesystem uses.
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".work");
    let tied_at = Utc::now();

    write_test_session_with_filename(
        &work_dir,
        "aaa-written-first-on-disk.md",
        SessionFixture {
            id: "session-z-tied",
            stage_id: "stage-tied",
            pid: std::process::id(),
            status: SessionStatus::Running,
            backend: SessionBackendKind::Tmux,
            created_at: tied_at,
            tracking_key: "loom-stage-tied-z",
        },
    );
    write_test_session_with_filename(
        &work_dir,
        "zzz-written-second-on-disk.md",
        SessionFixture {
            id: "session-a-tied",
            stage_id: "stage-tied",
            pid: std::process::id(),
            status: SessionStatus::Running,
            backend: SessionBackendKind::Tmux,
            created_at: tied_at,
            tracking_key: "loom-stage-tied-a",
        },
    );

    let sessions = live_tmux_sessions(&work_dir).unwrap();
    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["session-a-tied", "session-z-tied"],
        "with tied created_at, order must fall back to id, ascending — regardless of on-disk filename order"
    );
}

/// Exercised as the PAIR `attach_direct` itself calls, not through a
/// test-only wrapper: a wrapper would let the two halves drift apart from
/// the composition that actually ships.
#[test]
fn newest_for_stage_picks_the_newest_by_created_at() {
    let now = Utc::now();
    let oldest = stub_session("session-a", "stage-x", now - chrono::Duration::seconds(20));
    // b and c TIE on created_at; the pick must still be deterministic.
    let tied_first = stub_session("session-b", "stage-x", now - chrono::Duration::seconds(10));
    let tied_second = stub_session("session-c", "stage-x", now - chrono::Duration::seconds(10));
    // A NEWER session belonging to a DIFFERENT stage: the filter, not just
    // the pick, is what must exclude it.
    let other_stage = stub_session("session-d", "stage-y", now);
    let sessions = vec![oldest, tied_first, tied_second, other_stage];

    let matches = matches_for_stage(&sessions, "stage-x");
    let picked = pick_newest(&matches).expect("stage-x has live sessions");
    assert_eq!(
        picked.id, "session-c",
        "on a created_at tie, the pick must be deterministic"
    );
}

#[test]
fn newest_for_stage_returns_none_for_an_unknown_stage() {
    let sessions = vec![stub_session("session-a", "stage-x", Utc::now())];
    let matches = matches_for_stage(&sessions, "stage-unknown");
    assert!(pick_newest(&matches).is_none());
}
