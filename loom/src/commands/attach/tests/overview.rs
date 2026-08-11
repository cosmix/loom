use super::super::tests::stub_session;
use super::*;
use chrono::Utc;
use std::process::Command;
use tempfile::TempDir;

/// Shared with `tests/overview_hardening.rs`, which pins the one step this
/// module's tests only locate.
pub(super) fn panes(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("loom-session-{i}"), format!("loom-stage-{i}")))
        .collect()
}

/// Index of the single step whose verb is `verb`, panicking if it is absent.
pub(super) fn step_index(steps: &[Vec<String>], verb: &str) -> usize {
    steps
        .iter()
        .position(|s| s.get(2).map(String::as_str) == Some(verb))
        .unwrap_or_else(|| panic!("a {verb} step must be present"))
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
    let new_session_idx = step_index(&steps, "new-session");
    let remain_idx = step_index(&steps, "set-option");
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
    // (1) the split loop passing `Some(&panes[0])` instead of `Some(pane)`
    //     — the overview would tile session 0 into every pane instead of
    //     one pane per session.
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
fn attachable_panes_drops_sessions_whose_server_is_not_ready() {
    // The gap this closes: discovery admits `Spawning` sessions and judges
    // liveness by PID alone (deliberately — see `tmux_endpoint_ready`), so a
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
