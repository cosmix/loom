//! Pure unit tests for [`super::reconcile_steps`] — no tmux server needed.
//! Density matches `commands/attach/tests/overview.rs`: every test names
//! the real regression it prevents.

use super::*;

/// Build a [`PaneInfo`] without tmux. `socket` mirrors
/// `PaneInfo::session_socket`: `Some(_)` is an attributed loom pane, `None`
/// is the operator's own split.
fn pane(id: &str, dead: bool, socket: Option<&str>) -> PaneInfo {
    PaneInfo {
        id: id.to_string(),
        dead,
        session_socket: socket.map(str::to_string),
    }
}

/// `n` distinct `(session_socket, tmux_session)` pairs, in order.
fn desired_sessions(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("loom-session-{i}"), format!("loom-stage-{i}")))
        .collect()
}

/// Expected steps for a pass that kills exactly one pane (`pane_id`) and
/// retiles once — [`retile_step`] is the production helper this reuses.
fn kill_then_retile(pane_id: &str) -> Vec<Vec<String>> {
    vec![
        vec![
            "kill-pane".to_string(),
            "-t".to_string(),
            pane_id.to_string(),
        ],
        retile_step(),
    ]
}

#[test]
fn reconcile_steps_add_only_six_retiles_after_every_split() {
    // THE THRESHOLD THIS PINS: re-tiling once at the end hard-fails on the
    // SIXTH split (see `reconcile_steps`'s Rule 1 doc and
    // `doc/loom/knowledge/mistakes/tmux-backend.md`, "tmux Layout and Option
    // Traps"). A smaller pane count can never see that failure.
    let desired = desired_sessions(6);

    let steps = reconcile_steps(&[], &desired);

    let split_positions: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.first().map(String::as_str) == Some("split-window"))
        .map(|(i, _)| i)
        .collect();
    let select_layout_count = steps
        .iter()
        .filter(|s| s.first().map(String::as_str) == Some("select-layout"))
        .count();

    assert_eq!(split_positions.len(), 6, "one split-window per addition");
    assert_eq!(
        select_layout_count,
        split_positions.len(),
        "exactly one select-layout per split, none extra"
    );
    for idx in &split_positions {
        assert_eq!(
            steps[idx + 1].first().map(String::as_str),
            Some("select-layout"),
            "select-layout must immediately follow every split-window"
        );
    }
}

#[test]
fn reconcile_steps_wires_each_added_pane_to_its_own_session() {
    // Count-only assertions cannot see WHICH session ends up in which added
    // pane — an off-by-one over `desired` would keep every count intact
    // while silently duplicating one session and dropping another.
    let desired = desired_sessions(4);

    let steps = reconcile_steps(&[], &desired);

    let actual: Vec<&str> = steps
        .iter()
        .filter(|s| s.first().map(String::as_str) == Some("split-window"))
        .map(|s| {
            s.last()
                .expect("a split-window step carries a trailing pane command")
                .as_str()
        })
        .collect();
    let expected: Vec<String> = desired
        .iter()
        .map(|(socket, key)| crate::orchestrator::terminal::tmux::viewer::pane_command(socket, key))
        .collect();

    assert_eq!(
        actual,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "add i must attach to desired[i]'s own session, in order"
    );
}

#[test]
fn reconcile_steps_respawns_a_dead_pane_whose_socket_is_still_desired() {
    // THE GAP THIS CLOSES: an attach CLIENT can die while its inner server
    // lives on — the pane is dead but the session is still perfectly live.
    // Killing it would be wrong; only a respawn re-runs the recorded attach.
    let panes = vec![pane("%1", true, Some("loom-session-a"))];
    let desired = vec![("loom-session-a".to_string(), "loom-stage-a".to_string())];

    let steps = reconcile_steps(&panes, &desired);

    assert_eq!(
        steps,
        vec![vec![
            "respawn-pane".to_string(),
            "-t".to_string(),
            "%1".to_string()
        ]],
        "exactly one respawn, no split, no kill"
    );
}

#[test]
fn reconcile_steps_kills_a_dead_pane_whose_session_is_gone() {
    let panes = vec![
        pane("%1", true, Some("loom-session-gone")),
        pane("%2", false, Some("loom-session-live")),
    ];
    let desired = vec![(
        "loom-session-live".to_string(),
        "loom-stage-live".to_string(),
    )];

    let steps = reconcile_steps(&panes, &desired);

    assert_eq!(
        steps,
        kill_then_retile("%1"),
        "the dead orphaned pane is killed and the survivors are retiled once; \
         the live pane gets no step of its own"
    );
}

#[test]
fn reconcile_steps_floor_blocks_the_only_kill_when_nothing_replaces_it() {
    // THE FLOOR: killing the LAST pane kills the window, then the session,
    // then — despite `exit-empty off` — the whole viewer server. A single
    // dead pane with nothing queued to replace it must emit ZERO steps,
    // leaving that pane as the "nothing running" placeholder.
    let panes = vec![pane("%1", true, Some("loom-session-gone"))];

    let steps = reconcile_steps(&panes, &[]);

    assert!(
        steps.is_empty(),
        "the last pane must never be killed with nothing to replace it"
    );
}

#[test]
fn reconcile_steps_floor_keeps_the_first_dead_pane_as_placeholder() {
    let panes = vec![
        pane("%1", true, Some("loom-session-gone-a")),
        pane("%2", true, Some("loom-session-gone-b")),
    ];

    let steps = reconcile_steps(&panes, &[]);

    assert_eq!(
        steps,
        kill_then_retile("%2"),
        "the FIRST dead pane in pane order survives as the retained placeholder, \
         and the survivor is retiled once"
    );
}

#[test]
fn reconcile_steps_floor_does_not_bind_when_an_addition_guarantees_a_survivor() {
    let panes = vec![
        pane("%1", true, Some("loom-session-gone-a")),
        pane("%2", true, Some("loom-session-gone-b")),
    ];
    let desired = vec![("loom-session-new".to_string(), "loom-stage-new".to_string())];

    let steps = reconcile_steps(&panes, &desired);

    let kill_count = steps
        .iter()
        .filter(|s| s.first().map(String::as_str) == Some("kill-pane"))
        .count();
    assert_eq!(
        kill_count, 2,
        "an incoming add already guarantees a survivor, so both stale panes may go"
    );
}

#[test]
fn reconcile_steps_collapses_a_duplicate_pane_pair() {
    // THE RACE THIS COVERS: a `loom attach` rebuild racing a reconcile tick
    // can leave two panes pointed at the same session socket. The dead one
    // is the duplicate to remove; the live one is the keeper and must get
    // NO step at all (both halves asserted, or a bug that also respawns the
    // live pane would slip through a kill-only assertion).
    let panes = vec![
        pane("%1", true, Some("loom-session-dup")),
        pane("%2", false, Some("loom-session-dup")),
    ];
    let desired = vec![("loom-session-dup".to_string(), "loom-stage-dup".to_string())];

    let steps = reconcile_steps(&panes, &desired);

    assert_eq!(
        steps,
        kill_then_retile("%1"),
        "only the dead duplicate is killed and the survivors retiled once; \
         the live keeper gets no step at all"
    );
}

#[test]
fn reconcile_steps_leaves_operator_panes_untouched_but_counts_them_for_the_floor() {
    let panes = vec![
        pane("%1", false, None), // the operator's own split
        pane("%2", true, Some("loom-session-gone")),
    ];

    let steps = reconcile_steps(&panes, &[]);

    assert_eq!(
        steps,
        kill_then_retile("%2"),
        "the operator's pane must never appear as a step, but its presence \
         satisfies the floor so the dead loom pane can still be killed and \
         the survivors retiled once"
    );
}

#[test]
fn reconcile_steps_is_a_true_no_op_when_reality_already_matches_desired() {
    let panes = vec![
        pane("%1", false, Some("loom-session-a")),
        pane("%2", false, Some("loom-session-b")),
    ];
    let desired = vec![
        ("loom-session-a".to_string(), "loom-stage-a".to_string()),
        ("loom-session-b".to_string(), "loom-stage-b".to_string()),
    ];

    assert!(
        reconcile_steps(&panes, &desired).is_empty(),
        "no tmux command may run beyond the executor's own probe and list-panes"
    );
}

#[test]
fn reconcile_steps_orders_adds_before_respawns_before_kills() {
    // Adds must land FIRST so a concurrent add already guarantees a survivor
    // before any kill runs — the other order could momentarily empty the
    // window even though a replacement pane was already queued.
    let panes = vec![
        pane("%1", true, Some("loom-session-respawn")), // dead, still desired
        pane("%2", true, Some("loom-session-gone")),    // dead, no longer desired
    ];
    let desired = vec![
        (
            "loom-session-respawn".to_string(),
            "loom-stage-respawn".to_string(),
        ),
        ("loom-session-new".to_string(), "loom-stage-new".to_string()),
    ];

    let steps = reconcile_steps(&panes, &desired);

    let split_idx = steps
        .iter()
        .position(|s| s.first().map(String::as_str) == Some("split-window"))
        .expect("an add step must be present");
    let respawn_idx = steps
        .iter()
        .position(|s| s.first().map(String::as_str) == Some("respawn-pane"))
        .expect("a respawn step must be present");
    let kill_idx = steps
        .iter()
        .position(|s| s.first().map(String::as_str) == Some("kill-pane"))
        .expect("a kill step must be present");

    assert!(split_idx < respawn_idx, "adds must precede respawns");
    assert!(respawn_idx < kill_idx, "respawns must precede kills");
}

#[test]
fn reconcile_steps_retiles_once_after_kills_and_never_without_them() {
    // Kills leave the SURVIVING panes at their pre-kill tiled geometry;
    // adds already retile themselves after every split (Rule 1), so only a
    // kill needs the trailing retile, and it must fire exactly once no
    // matter how many panes are killed in the same pass.
    let panes = vec![
        pane("%1", true, Some("loom-session-gone-a")),
        pane("%2", true, Some("loom-session-gone-b")),
        pane("%3", false, Some("loom-session-live")),
    ];
    let desired = vec![(
        "loom-session-live".to_string(),
        "loom-stage-live".to_string(),
    )];

    let steps = reconcile_steps(&panes, &desired);

    let select_layout_count = steps
        .iter()
        .filter(|s| s.first().map(String::as_str) == Some("select-layout"))
        .count();
    assert_eq!(
        select_layout_count, 1,
        "exactly one retile no matter how many panes were killed"
    );
    assert_eq!(
        steps.last(),
        Some(&retile_step()),
        "the retile must be the LAST step, after every kill"
    );

    // No kills at all -> no trailing retile.
    let no_kills = vec![pane("%1", true, Some("loom-session-live"))];
    let steps_no_kill = reconcile_steps(&no_kills, &desired);
    assert!(
        steps_no_kill
            .iter()
            .all(|s| s.first().map(String::as_str) != Some("select-layout")),
        "no retile may appear when nothing was killed"
    );
}
