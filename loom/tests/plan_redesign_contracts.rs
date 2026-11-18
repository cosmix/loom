//! Contracts of stage session-and-wire (PLAN-feat-planner-13-planner-redesign): ask rules, blocked
//! actions in the spool, the owner watch, and the slug-named six-step checklist wire.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use loom::plan_server::lock::{acquire, Acquired};
use loom::plan_server::wire::checklist::{ChecklistView, ProjectChecklistView};
use loom::plan_session::hook_cli;
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::owner_watch::{self, read_holder_pid, OwnerProbe};
use loom::plan_session::sandbox::{write_session_sandbox, SandboxInputs};
use loom::plan_session::scope::{self, SessionScope};
use loom::plan_session::spool::{SpoolReader, SPOOL_EVENTS};
use loom::planning::project::Project;
use loom::planning::runtime::runtime_paths_under;
use serde_json::Value;
use tempfile::TempDir;

/// The documented `PermissionDenied` hook payload (`code.claude.com/docs/en/hooks`).
const DENIED_PAYLOAD: &str = r#"{"session_id":"abc123","transcript_path":"/x.jsonl","cwd":"/x","permission_mode":"auto","hook_event_name":"PermissionDenied","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/build","description":"Clean build directory"},"tool_use_id":"toolu_01ABC","reason":"[Irreversible Local Destruction]"}"#;

const KICKOFF_ASKS: [&str; 4] = [
    "Bash(loom knowledge *)",
    "Bash(loom plan summary seal*)",
    "Bash(loom plan summary brief*)",
    "Bash(loom plan summary release*)",
];
const SUMMARY_STATUS: &str = "Bash(loom plan summary status*)";

/// The canonical base of a fresh temporary directory.
fn scratch() -> (TempDir, PathBuf) {
    let temp = tempfile::Builder::new()
        .prefix("loom-redesign-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let base = temp.path().canonicalize().unwrap();
    (temp, base)
}

/// The `settings.json` written for an authoring session in Kickoff scope.
fn kickoff_settings() -> Value {
    let (_temp, base) = scratch();
    let root = base.join("proj");
    let home = base.join("home");
    fs::create_dir_all(root.join("doc/plans")).unwrap();
    fs::create_dir_all(root.join("doc/product")).unwrap();
    fs::create_dir_all(&home).unwrap();
    let runtime = runtime_paths_under(&home.join(".loom"), &root).unwrap();
    let session = SessionDir::new(runtime.sessions.join("s1"));
    fs::create_dir_all(session.path()).unwrap();
    fs::set_permissions(session.path(), fs::Permissions::from_mode(0o700)).unwrap();
    scope::write(&session, &SessionScope::kickoff(&root).unwrap()).unwrap();
    let loom_bin = PathBuf::from(env!("CARGO_BIN_EXE_loom"));
    let inputs = SandboxInputs {
        project_root: &root,
        runtime: &runtime,
        session_dir: &session,
        kind: &SessionKind::Authoring,
        loom_bin: &loom_bin,
        home: &home,
    };
    let files = write_session_sandbox(&inputs, &[]).unwrap();
    serde_json::from_str(&fs::read_to_string(files.settings).unwrap()).unwrap()
}

/// The strings of the permission list `name`.
fn rules(settings: &Value, name: &str) -> Vec<String> {
    let list = settings["permissions"][name].as_array().unwrap();
    let text = list.iter().map(|rule| rule.as_str().unwrap().to_owned());
    text.collect()
}

#[test]
fn kickoff_writers_ask_the_holder() {
    let settings = kickoff_settings();
    let (ask, allow) = (rules(&settings, "ask"), rules(&settings, "allow"));
    for rule in KICKOFF_ASKS {
        assert!(ask.iter().any(|r| r == rule), "{rule} not in ask: {ask:?}");
        let allowed = allow.iter().any(|r| r == rule);
        assert!(!allowed, "{rule} must not be allowed: {allow:?}");
    }
    // Ask outranks allow: a broad summary ask or a status ask stops the pre-allowed status command.
    for rule in ["Bash(loom plan summary *)", SUMMARY_STATUS] {
        assert!(!ask.iter().any(|r| r == rule), "{rule} in ask: {ask:?}");
    }
    let status = allow.iter().any(|r| r == SUMMARY_STATUS);
    assert!(status, "{SUMMARY_STATUS} not in allow: {allow:?}");
}

#[test]
fn denied_action_spools_silently() {
    let (_temp, base) = scratch();
    let session = SessionDir::new(base.join("s1"));
    fs::create_dir(session.path()).unwrap();
    let mut stdout = Vec::new();
    let code = hook_cli::run(
        "PermissionDenied",
        session.path(),
        DENIED_PAYLOAD.as_bytes(),
        &mut stdout,
    );
    assert_eq!(code, 0);
    let printed = String::from_utf8_lossy(&stdout);
    assert!(printed.is_empty(), "the hook answered: {printed}");
    let events = SpoolReader::new(session.events()).read_new();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].event, "PermissionDenied");
    assert_eq!(events[0].payload["tool_name"], "Bash");
    assert_eq!(
        events[0].payload["reason"],
        "[Irreversible Local Destruction]"
    );
    let registered = SPOOL_EVENTS.iter().any(|h| h.event == "PermissionDenied");
    assert!(registered, "Claude Code never calls an unregistered event");
}

/// A world on a fake clock: every answer follows the seconds elapsed, so the order and number of
/// the watch's questions within a poll do not matter.
struct World {
    start: Instant,
    elapsed: Duration,
    /// The owner is alive over these elapsed seconds, end excluded.
    owner_seen: Option<(u64, u64)>,
    /// The session ends by itself at this elapsed second.
    session_ends: Option<u64>,
    /// The elapsed second of each teardown.
    teardowns: Vec<u64>,
    /// Questions asked: a watch that never sleeps fails instead of hanging the test.
    questions: u32,
}

impl World {
    fn new(owner_seen: Option<(u64, u64)>, session_ends: Option<u64>) -> Self {
        World {
            start: Instant::now(),
            elapsed: Duration::ZERO,
            owner_seen,
            session_ends,
            teardowns: Vec::new(),
            questions: 0,
        }
    }

    fn second(&mut self) -> u64 {
        self.questions += 1;
        assert!(self.questions < 100_000, "the watch does not sleep");
        self.elapsed.as_secs()
    }
}

impl OwnerProbe for World {
    fn owner_alive(&mut self) -> bool {
        let second = self.second();
        self.owner_seen
            .is_some_and(|(from, to)| (from..to).contains(&second))
    }

    fn session_alive(&mut self) -> bool {
        let second = self.second();
        self.teardowns.is_empty() && self.session_ends.is_none_or(|end| second < end)
    }

    fn teardown(&mut self) {
        let second = self.second();
        self.teardowns.push(second);
    }

    fn sleep(&mut self, duration: Duration) {
        self.elapsed += duration;
        let hour = Duration::from_secs(3600);
        assert!(self.elapsed < hour, "the watch never returned");
    }

    fn now(&mut self) -> Instant {
        self.second();
        self.start + self.elapsed
    }
}

const GRACE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_secs(15);

#[test]
fn owner_watch_tears_down_after_grace() {
    let mut world = World::new(None, None);
    owner_watch::run(&mut world, GRACE, POLL);
    assert_eq!(world.teardowns.len(), 1, "teardowns: {:?}", world.teardowns);
    let at = world.teardowns[0];
    assert!(at >= 60, "torn down at {at} s, inside the 60 s grace");
    assert!(at <= 75, "torn down at {at} s, a poll past the 60 s grace");
}

#[test]
fn owner_return_keeps_session() {
    // Gone for 45 s, back for one poll, gone for 45 s, then the session ends by itself. No absence
    // reaches the grace; the two together do.
    let mut world = World::new(Some((45, 60)), Some(90));
    owner_watch::run(&mut world, GRACE, POLL);
    assert!(
        world.teardowns.is_empty(),
        "torn down at {:?} though the owner came back at 45 s",
        world.teardowns
    );
    let ended = world.elapsed.as_secs();
    assert!(ended >= 90, "returned at {ended} s, with the session alive");
    assert!(ended <= 105, "returned at {ended} s, a poll past its end");
}

#[test]
fn holder_read_takes_no_lock() {
    let (_temp, base) = scratch();
    let path = base.join("server.lock");
    assert_eq!(read_holder_pid(&path), None, "a missing file has no holder");
    let Acquired::Held(lock) = acquire(&path).unwrap() else {
        panic!("a fresh lock file must be acquired");
    };
    assert_eq!(read_holder_pid(&path), Some(std::process::id()));
    drop(lock);
    // The record stays after the holder goes, and a reader leaves the lock free for a new server.
    assert_eq!(read_holder_pid(&path), Some(std::process::id()));
    assert!(
        matches!(acquire(&path).unwrap(), Acquired::Held(_)),
        "a read must leave the lock free for a starting server"
    );
}

/// A temporary repository root holding `files`.
fn tree(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let (temp, base) = scratch();
    fs::create_dir_all(base.join("doc")).unwrap();
    for (rel, text) in files {
        let path = base.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    (temp, base)
}

/// The converted checklist of `feature` in the tree at `root`.
fn card(root: &Path, feature: &str) -> ChecklistView {
    let project = Project::load(root).unwrap();
    let view = ProjectChecklistView::from(&project);
    let names: Vec<_> = view.checklists.iter().map(|c| c.feature.clone()).collect();
    let found = view.checklists.into_iter().find(|c| c.feature == feature);
    found.unwrap_or_else(|| panic!("no checklist for {feature}; features: {names:?}"))
}

#[test]
fn checklist_has_no_title_and_six_steps() {
    let vision = "# Atlas Vision\n\nA planner for atlases.\n";
    let (_temp, root) = tree(&[("doc/product/atlas/index.md", vision)]);
    let view = card(&root, "atlas");
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["feature"], "atlas");
    assert!(json.get("title").is_none(), "a title on the wire: {json}");
    let numbers: Vec<u8> = view.steps.iter().map(|s| s.number).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 5, 6]);
    let names: Vec<&str> = view.steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        !names.contains(&"Review"),
        "Review is still a step: {names:?}"
    );
    assert_eq!(names.last(), Some(&"Run"), "{names:?}");
}

#[test]
fn finished_feature_has_no_primary() {
    let vision = "# Demo\n\nA demo.\n\n## Specification sections\n\n\
                  | # | Section |\n| --- | --- |\n| 01 | Alpha |\n";
    let meta = "# Metaplan\n\n## 1. A (`PLAN-a.md`)\n\n## 2. B (`PLAN-b.md`)\n";
    let (_temp, root) = tree(&[
        ("doc/product/demo/index.md", vision),
        ("doc/product/demo/section_01.md", "# Alpha\n\nBody.\n"),
        ("doc/plans/demo/index.md", meta),
        ("doc/plans/demo/archive/DONE-PLAN-a.md", "# A\n"),
        ("doc/plans/demo/archive/DONE-PLAN-b.md", "# B\n"),
    ]);
    let view = card(&root, "demo");
    assert_eq!(view.primary.as_ref().map(|o| o.id.as_str()), None);
    let secondary: Vec<&str> = view.secondary.iter().map(|o| o.id.as_str()).collect();
    assert!(
        secondary.contains(&"plan-follow-ups"),
        "plan-follow-ups missing from {secondary:?}"
    );
}

#[test]
fn plan_rows_carry_pressure_date() {
    let meta = "# Metaplan\n\n## 1. A (`PLAN-a.md`)\n\n## 2. B (`PLAN-b.md`)\n";
    // Out of date order: the row shows the latest record, neither the first nor the last.
    let tested = "# A\n\n## Pressure tests\n\n- 2025-01-01: a\n- 2025-03-03: b\n- 2025-02-02: c\n";
    let (_temp, root) = tree(&[
        ("doc/plans/demo/index.md", meta),
        ("doc/plans/demo/PLAN-a.md", tested),
        ("doc/plans/demo/PLAN-b.md", "# B\n"),
        ("doc/plans/demo/PLAN-c.md", "# C\n"),
    ]);
    let view = card(&root, "demo");
    let metaplans: Vec<&str> = view.metaplans.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(metaplans, ["doc/plans/demo/index.md"]);
    let rows: Vec<(&str, &str, Option<&str>, bool)> = view
        .plan_rows
        .iter()
        .map(|row| {
            let date = row.pressure_date.as_deref();
            (row.slug.as_str(), row.path.as_str(), date, row.listed)
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("a", "doc/plans/demo/PLAN-a.md", Some("2025-03-03"), true),
            ("b", "doc/plans/demo/PLAN-b.md", None, true),
            ("c", "doc/plans/demo/PLAN-c.md", None, false),
        ]
    );
    let states: Vec<&str> = view.plan_rows.iter().map(|r| r.state.as_str()).collect();
    assert_eq!(states, ["written", "written", "written"]);
}
