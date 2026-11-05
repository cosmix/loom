//! The confinement fixture: a fake home, a git project and a planning runtime, with an
//! authoring session's sandbox files written by `write_session_sandbox`.
//!
//! Everything lives in a `tempfile::Builder::new().prefix("loom-plan-session-")` directory
//! under the system temp dir, as the plan_session sandbox contracts do.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::sandbox::{ensure_runtime_grants, write_session_sandbox, SandboxInputs};
use loom::planning::runtime::{runtime_paths_under, RuntimePaths};
use serde_json::Value;
use tempfile::TempDir;

/// The built loom binary. Under srt the `LOOM_*` scrub is done by `Confined::run`; this test
/// never spawns the binary from the test process itself.
pub const LOOM: &str = env!("CARGO_BIN_EXE_loom");
/// A session id `LOOM_PLAN_SESSION` accepts (8 to 64 hex digits or dashes).
pub const SESSION_ID: &str = "0b8f6c2e-4d1a-4e8b-9c3f-2a7d5e6f1b90";
/// The summary the fixture's draft seals.
pub const SUMMARY_ID: &str = "plans/a";
/// A running plan present at build time: locked, so its path is in `denyWrite`.
pub const LOCKED_PLAN: &str = "doc/plans/IN_PROGRESS-PLAN-b.md";
/// The content of every file a deny probe must not read.
pub const SECRET: &str = "planning-confinement-secret";
/// The symbol `loom map --outline loom/src/lib.rs` must print.
pub const SYMBOL: &str = "confinement_probe_symbol";
/// A two-level summary draft of `doc/plans/PLAN-a.md`: an opening sentence citing the detail
/// sentence, which cites the plan's title section.
const DRAFT: &str =
    "# A\n\nA is planned. [d](#s-ee726dbe)\n\nA does a. [§](../../../plans/PLAN-a.md#plan-a)\n";

/// One project with its runtime and the authoring session's sandbox files.
pub struct Fixture {
    _tmp: TempDir,
    /// The canonical temp directory holding the project, the home and the srt settings.
    pub base: PathBuf,
    pub root: PathBuf,
    pub home: PathBuf,
    pub loom_home: PathBuf,
    pub runtime: RuntimePaths,
    pub session: SessionDir,
    /// The authoring `settings.json` `write_session_sandbox` wrote.
    pub settings: Value,
}

pub fn fixture() -> Fixture {
    let tmp = tempfile::Builder::new()
        .prefix("loom-plan-session-")
        .tempdir_in(std::env::temp_dir())
        .expect("create the fixture directory");
    let base = tmp.path().canonicalize().unwrap();
    let (root, home) = (base.join("proj"), base.join("home"));
    let loom_home = home.join(".loom");
    populate_project(&root, &home);
    let runtime = runtime_paths_under(&loom_home, &root).unwrap();
    let session = open_session(&runtime);
    let settings = write_settings(&root, &home, &runtime, &session);
    Fixture {
        _tmp: tmp,
        base,
        root,
        home,
        loom_home,
        runtime,
        session,
        settings,
    }
}

fn populate_project(root: &Path, home: &Path) {
    let files = [
        ("loom/src/lib.rs", format!("pub fn {SYMBOL}() {{}}\n")),
        ("README.md", "# Project\n".to_string()),
        ("doc/product/index.md", "# Vision\n\nText.\n".to_string()),
        ("doc/plans/PLAN-a.md", "# Plan: A\n\nDoes a.\n".to_string()),
        (LOCKED_PLAN, "# Plan: B\n\nDoes b.\n".to_string()),
        (".loom/work/admin.token", SECRET.to_string()),
    ];
    for (rel, body) in files {
        put(&root.join(rel), &body);
    }
    put(&home.join(".ssh/id_rsa"), SECRET);
    git_commit(root, home);
}

/// `git init` and one commit of the tracked content: `loom map` needs a `HEAD`. The tokens
/// stay untracked. A scratch `HOME` keeps the user's git configuration out.
fn git_commit(root: &Path, home: &Path) {
    let identity = [
        "-c",
        "user.name=loom-test",
        "-c",
        "user.email=loom@example.invalid",
    ];
    let steps: [&[&str]; 3] = [
        &["init", "-q"],
        &["add", "loom", "README.md", "doc"],
        &["commit", "-q", "-m", "fixture"],
    ];
    for step in steps {
        let output = Command::new("git")
            .args(identity)
            .args(step)
            .current_dir(root)
            .env("HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {step:?}: {output:?}");
    }
}

/// The runtime grants, the session directory, a deny-probe file under each runtime area and
/// the summary draft `loom plan summary seal` seals.
fn open_session(runtime: &RuntimePaths) -> SessionDir {
    ensure_runtime_grants(runtime).unwrap();
    let session = SessionDir::new(runtime.sessions.join(SESSION_ID));
    fs::create_dir_all(session.path()).unwrap();
    for dir in [&runtime.sessions, session.path()] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    put(&runtime.lock, SECRET);
    put(&session.answers().join("a.json"), SECRET);
    put(&runtime.drafts.join(format!("{SUMMARY_ID}.md")), DRAFT);
    session
}

fn write_settings(root: &Path, home: &Path, runtime: &RuntimePaths, session: &SessionDir) -> Value {
    let kind = SessionKind::Authoring;
    let inputs = SandboxInputs {
        project_root: root,
        runtime,
        session_dir: session,
        kind: &kind,
        loom_bin: Path::new(LOOM),
        home,
    };
    let files = write_session_sandbox(&inputs, &[]).expect("write the session sandbox");
    let text = fs::read_to_string(files.settings).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn put(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}
