use super::*;
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
