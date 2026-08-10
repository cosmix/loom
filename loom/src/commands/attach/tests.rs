use super::*;
use chrono::{DateTime, Utc};
use tempfile::TempDir;

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
/// touch the filesystem or tmux. Shared with `tests/overview.rs`, whose
/// `attachable_panes` tests need the same fixture.
pub(super) fn stub_session(id: &str, stage_id: &str, created_at: DateTime<Utc>) -> Session {
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
